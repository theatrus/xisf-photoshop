// Load the built .8bi files and exercise the real Adobe SDK ABI with a small
// deterministic host. This supplements, rather than replaces, Photoshop tests.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#ifdef _WIN32
#include <windows.h>
#else
#include <CoreFoundation/CoreFoundation.h>
#include <CoreServices/CoreServices.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <unistd.h>
#endif
#include "PIDefines.h"
#include "PIFormat.h"
#include "PIProperties.h"
#include "seiza_codec.h"
#include "host_file.h"
#include "options.h"
#include "preferences.h"
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

using Entry = void (*)(int16, FormatRecord*, intptr_t*, int16*);
struct TestHandle { std::vector<char> bytes; };
static Handle MACPASCAL newHandle(int32 size) {
    return reinterpret_cast<Handle>(new TestHandle{std::vector<char>(size)});
}
static void MACPASCAL disposeHandle(Handle handle) { delete reinterpret_cast<TestHandle*>(handle); }
static int32 MACPASCAL handleSize(Handle handle) {
    return static_cast<int32>(reinterpret_cast<TestHandle*>(handle)->bytes.size());
}
static Ptr MACPASCAL lockHandle(Handle handle, Boolean) {
    return reinterpret_cast<Ptr>(reinterpret_cast<TestHandle*>(handle)->bytes.data());
}
static void MACPASCAL unlockHandle(Handle) {}
static HandleProcs handles = [] {
    HandleProcs h{};
    h.handleProcsVersion = 1; h.numHandleProcs = kCurrentHandleProcsCount;
    h.newProc = newHandle; h.disposeProc = disposeHandle; h.getSizeProc = handleSize;
    h.lockProc = lockHandle; h.unlockProc = unlockHandle;
    return h;
}();
struct Host {
    FormatRecord record{};
    intptr_t state = 0;
    Str255 error{};
    Entry entry;
    std::vector<float> pixels;
    std::vector<char> xmp;
    bool writing = false;
    int transfers = 0;
    int cancelAfter = -1;
    PIDescriptorParameters descriptor{};
    ~Host() {
        if (record.revertInfo) disposeHandle(record.revertInfo);
        if (record.iCCprofileData) disposeHandle(record.iCCprofileData);
    }
};
static Host* current = nullptr;
static OSErr MACPASCAL getProperty(PIType signature, PIType key, int32, intptr_t*, Handle* output) {
    if (signature != kPhotoshopSignature || key != propXMP || !output) return formatBadParameters;
    *output = newHandle(static_cast<int32>(current->xmp.size()));
    reinterpret_cast<TestHandle*>(*output)->bytes = current->xmp;
    return noErr;
}
static OSErr MACPASCAL setProperty(PIType signature, PIType key, int32, intptr_t, Handle value) {
    if (signature != kPhotoshopSignature || key != propXMP || !value) return formatBadParameters;
    current->xmp = reinterpret_cast<TestHandle*>(value)->bytes;
    return noErr;
}
static PropertyProcs properties = [] {
    PropertyProcs p{};
    p.propertyProcsVersion = kCurrentPropertyProcsVersion;
    p.numPropertyProcs = kCurrentPropertyProcsCount;
    p.getPropertyProc = getProperty; p.setPropertyProc = setProperty;
    return p;
}();
static void require(bool value, const char* message) {
    if (!value) throw std::runtime_error(message);
}
static Boolean MACPASCAL abortProc() {
    return current->cancelAfter >= 0 && current->transfers >= current->cancelAfter;
}
static OSErr MACPASCAL advanceProc() {
    auto& h = *current;
    auto& r = h.record;
    require((r.depth == 32 || r.depth == 16) && r.data && r.colBytes == r.depth / 8, "Invalid host pixel transfer");
    require(r.rowBytes == r.imageSize32.h * r.colBytes, "Incorrect row stride");
    require(r.theRect32.left == 0 && r.theRect32.right == r.imageSize32.h &&
        r.theRect32.bottom == r.theRect32.top + 1 && r.loPlane == r.hiPlane, "Invalid rectangle/plane");
    const size_t offset = (static_cast<size_t>(r.loPlane) * r.imageSize32.v + r.theRect32.top) * r.imageSize32.h;
    require(offset + r.imageSize32.h <= h.pixels.size(), "Transfer outside image");
    if (r.depth == 32) {
        auto* samples = static_cast<float*>(r.data);
        if (h.writing) std::copy_n(h.pixels.data() + offset, r.imageSize32.h, samples);
        else std::copy_n(samples, r.imageSize32.h, h.pixels.data() + offset);
    } else {
        auto* samples = static_cast<uint16_t*>(r.data);
        for (int32 x = 0; x < r.imageSize32.h; ++x) {
            if (h.writing) samples[x] = static_cast<uint16_t>(std::lround(h.pixels[offset + x] * 32768.0));
            else {
                require(r.maxValue == 32768 && samples[x] <= 32768, "Wrong Photoshop 16-bit range");
                h.pixels[offset + x] = samples[x] / 32768.0f;
            }
        }
    }
    ++h.transfers;
    return noErr;
}
static int32_t append(void* context, const uint8_t* bytes, size_t count) {
    auto& v = *static_cast<std::vector<uint8_t>*>(context);
    v.insert(v.end(), bytes, bytes + count);
    return 0;
}
static int16 call(Host& h, int16 selector) {
    current = &h;
    int16 result = noErr;
    h.entry(selector, &h.record, &h.state, &result);
    return result;
}
static void success(Host& h, int16 selector) {
    const auto result = call(h, selector);
    if (result) throw std::runtime_error("Selector " + std::to_string(selector) + " failed: " +
        std::to_string(result) + " " + std::string(reinterpret_cast<char*>(h.error + 1), h.error[0]));
}
#ifdef _WIN32
using File = HANDLE;
using Module = HMODULE;
#else
using File = int;
using Module = void*;
#endif
static void initialize(Host& h, File file) {
#ifdef _WIN32
    h.record.dataFork = reinterpret_cast<intptr_t>(file);
#else
    h.record.posixFileDescriptor = file;
    h.record.hostSupportsPOSIXIO = true;
    h.record.pluginUsingPOSIXIO = true;
#endif
    h.record.HostSupports32BitCoordinates = true;
    h.record.abortProc = abortProc;
    h.record.advanceState = advanceProc;
    h.record.errorString = &h.error;
    h.descriptor.playInfo = plugInDialogSilent;
    h.record.descriptorParameters = &h.descriptor;
    h.record.handleProcs = &handles;
    h.record.propertyProcs = &properties;
    h.record.canUseICCProfiles = true;
}
static void setOptions(Host& h, uint32_t readDepth, uint32_t writeDepth) {
    SeizaOptions options;
    options.readDepth = readDepth; options.writeDepth = writeDepth;
    if (h.record.revertInfo) disposeHandle(h.record.revertInfo);
    h.record.revertInfo = newHandle(sizeof(options));
    std::memcpy(lockHandle(h.record.revertInfo, false), &options, sizeof(options));
}
static void closeFile(File file) {
#ifdef _WIN32
    CloseHandle(file);
#else
    close(file);
#endif
}
static void makeCfaFixture(uint32_t format, uint32_t width, uint32_t height,
    const std::vector<float>& expected, std::vector<uint8_t>& encoded, bool tagged) {
        if (format == 1) {
            size_t end = 0;
            while (end + 240 < encoded.size() && std::memcmp(encoded.data() + end, "END     ", 8)) end += 80;
            require(end + 240 < 2880, "Missing FITS fixture header space");
            for (const char* card : {tagged ? "BAYERPAT= 'RGGB'" : "COMMENT No Bayer pattern", "XBAYROFF=                    1", "END"}) {
                std::fill_n(encoded.data() + end, 80, static_cast<uint8_t>(' '));
                std::memcpy(encoded.data() + end, card, std::strlen(card)); end += 80;
            }
        } else {
            const auto xml = std::string("<xisf version=\"1.0\" xmlns=\"http://www.pixinsight.com/xisf\"><Image geometry=\"") +
                std::to_string(width) + ":" + std::to_string(height) + ":1\" sampleFormat=\"Float32\" colorSpace=\"Gray\" location=\"attachment:4096:" +
                std::to_string(expected.size() * 4) + "\">" +
                (tagged ? "<ColorFilterArray pattern=\"RGGB\" width=\"2\" height=\"2\"/>" : "") +
                "<FITSKeyword name=\"XBAYROFF\" value=\"1\"/></Image></xisf>";
            encoded.assign(4096, 0); std::memcpy(encoded.data(), "XISF0100", 8);
            const uint32_t length = static_cast<uint32_t>(xml.size());
            for (int i = 0; i < 4; ++i) encoded[8 + i] = static_cast<uint8_t>(length >> (i * 8));
            std::memcpy(encoded.data() + 16, xml.data(), xml.size());
            for (float value : expected) {
                uint32_t bits; std::memcpy(&bits, &value, 4);
                for (int i = 0; i < 4; ++i) encoded.push_back(static_cast<uint8_t>(bits >> (i * 8)));
            }
        }
}

static std::vector<uint8_t> profileFixture(uint32_t planes, uint8_t marker) {
    std::vector<uint8_t> bytes(132, 0);
    bytes[3] = 132;
    std::memcpy(bytes.data() + 16, planes == 1 ? "GRAY" : "RGB ", 4);
    std::memcpy(bytes.data() + 36, "acsp", 4);
    bytes[80] = marker;
    return bytes;
}
static void addMetadataFixture(uint32_t format, std::vector<uint8_t>& encoded, uint32_t planes) {
    if (format == 1) {
        size_t end = 0;
        while (end + 240 < encoded.size() && std::memcmp(encoded.data() + end, "END     ", 8)) end += 80;
        require(end + 240 < 2880, "Missing metadata fixture header space");
        for (const char* card : {"OBJECT  = 'metadata-document-A' / original target", "EXPTIME =                  180 / seconds", "END"}) {
            std::fill_n(encoded.data() + end, 80, static_cast<uint8_t>(' '));
            std::memcpy(encoded.data() + end, card, std::strlen(card)); end += 80;
        }
    } else {
        const uint32_t length = encoded[8] | (encoded[9] << 8) | (encoded[10] << 16) | (encoded[11] << 24);
        std::string xml(reinterpret_cast<const char*>(encoded.data() + 16), length);
        const std::string fields = "<FITSKeyword name=\"OBJECT\" value=\"'metadata-document-A'\" comment=\"original target\"/>"
            "<FITSKeyword name=\"EXPTIME\" value=\"180\" comment=\"seconds\"/>"
            "<ICCProfile location=\"attachment:" + std::to_string(encoded.size()) + ":132\"/>";
        const auto profile = profileFixture(planes, 1);
        encoded.insert(encoded.end(), profile.begin(), profile.end());
        const auto end = xml.find("</Image>");
        if (end != std::string::npos) xml.insert(end, fields);
        else {
            const auto close = xml.find("/>", xml.find("<Image"));
            require(close != std::string::npos, "Missing XISF fixture image");
            xml.replace(close, 2, ">" + fields + "</Image>");
        }
        while (!xml.empty() && (xml.back() == ' ' || xml.back() == '\0')) xml.pop_back();
        require(xml.size() < 4080, "Metadata fixture exceeds header block");
        const auto newLength = static_cast<uint32_t>(xml.size());
        for (int i = 0; i < 4; ++i) encoded[8 + i] = static_cast<uint8_t>(newLength >> (i * 8));
        std::fill(encoded.begin() + 16, encoded.begin() + 4096, 0);
        std::memcpy(encoded.data() + 16, xml.data(), xml.size());
    }
}

static void run(Module module, uint32_t format, uint32_t width, uint32_t height, uint32_t planes,
    uint32_t readDepth, uint32_t writeDepth, bool interactive = false, bool useDefaults = false,
    bool changeMode = false, bool legacyOptions = false, uint32_t cfaMode = UINT32_MAX, bool tagged = true) {
#ifdef _WIN32
    auto entry = reinterpret_cast<Entry>(GetProcAddress(module, "PluginMain"));
    require(FindResourceW(module, MAKEINTRESOURCEW(16000), L"PiPL") != nullptr, "PiPL resource missing");
    require(FindResourceW(module, MAKEINTRESOURCEW(SEIZA_OPTIONS_DIALOG), MAKEINTRESOURCEW(5)) != nullptr, "Options dialog resource missing");
    require(FindResourceW(module, MAKEINTRESOURCEW(SEIZA_SETTINGS_DIALOG), MAKEINTRESOURCEW(5)) != nullptr, "Settings dialog resource missing");
#else
    auto entry = reinterpret_cast<Entry>(dlsym(module, "PluginMain"));
#endif
    require(entry != nullptr, "PluginMain export is missing");
    std::vector<float> expected(static_cast<size_t>(width) * height * planes);
    for (size_t i = 0; i < expected.size(); ++i) expected[i] = static_cast<float>(i % 37) / 8.0f - 0.25f;
    if (width == 5 && height == 1 && planes == 1) expected = {-std::numeric_limits<float>::max(), -1.0f, 0.0f, 1.0f, std::numeric_limits<float>::max()};
    if (width == 4 && height == 1 && planes == 1) expected = {1.0e-40f, 2.0e-40f, 4.0e-40f, 8.0e-40f};
    if (cfaMode != UINT32_MAX) {
        require(planes == 1, "CFA test must start with a single plane");
        for (uint32_t y = 0; y < height; ++y) for (uint32_t x = 0; x < width; ++x)
            expected[y * width + x] = y % 2 == 0 ? (x % 2 == 0 ? 0.5f : -0.25f) : (x % 2 == 0 ? 2.0f : 0.5f);
    }
    std::vector<uint8_t> encoded;
    char error[1024]{};
    require(seiza_encode(format, width, height, planes, expected.data(), expected.size(), append, &encoded, error, sizeof(error)) == 0, error);
    if (cfaMode != UINT32_MAX) {
        makeCfaFixture(format, width, height, expected, encoded, tagged);
        if (cfaMode && (tagged || cfaMode > 1)) {
            planes = 3;
            expected.clear();
            for (float value : {-0.25f, 0.5f, 2.0f}) expected.insert(expected.end(), static_cast<size_t>(width) * height, value);
        }
    }
    // A Unicode path also verifies that plug-in I/O only uses host file handles.
    addMetadataFixture(format, encoded, planes);
#ifdef _WIN32
    File file = CreateFileW(L"build\\native\\host-\u661f.tmp", GENERIC_READ | GENERIC_WRITE,
        FILE_SHARE_READ, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE, nullptr);
    require(file != INVALID_HANDLE_VALUE, "Cannot create host test file");
#else
    const char* path = "build/native-macos/host-\u661f.tmp";
    File file = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    require(file >= 0, "Cannot create host test file");
    unlink(path);
#endif
    try {
        Host reader{}; reader.entry = entry; initialize(reader, file);
        if (interactive || useDefaults) reader.descriptor.playInfo = plugInDialogDisplay;
        else setOptions(reader, readDepth, 32);
        if (cfaMode != UINT32_MAX && !interactive && !useDefaults) {
            setOptions(reader, readDepth, 0);
            reinterpret_cast<SeizaOptions*>(lockHandle(reader.record.revertInfo, false))->debayer = cfaMode;
        }
        HostFile io(reader.record);
        require(io.write(encoded.data(), encoded.size()) == encoded.size(), "Fixture write failed");
        io.seek(7);
        success(reader, formatSelectorFilterFile);
        require(io.position() == 7, "Filter did not preserve file position");
        success(reader, formatSelectorReadPrepare);
        success(reader, formatSelectorReadStart);
        require(reader.record.imageSize32.h == width && reader.record.imageSize32.v == height &&
            reader.record.planes == planes && reader.record.depth == readDepth, "Incorrect image description");
        require(reader.record.imageMode == (planes == 3 ? plugInModeRGBColor : plugInModeGrayScale), "Incorrect Photoshop color mode");
        reader.pixels.resize(expected.size());
        if (readDepth == 16) {
            const double low = *std::min_element(expected.begin(), expected.end());
            const double high = *std::max_element(expected.begin(), expected.end());
            for (auto& value : expected)
                value = high == low ? 0.0f : static_cast<float>(std::round(
                    ((static_cast<double>(value) - low) / (high - low)) * 32768.0) / 32768.0);
        }
        success(reader, formatSelectorReadContinue);
        require(reader.pixels == expected && reader.record.data == nullptr, "Host received incorrect pixels");
        success(reader, formatSelectorReadFinish);
        if (format == 2) {
            const auto wanted = profileFixture(planes, 1);
            require(reader.record.iCCprofileData && reader.record.iCCprofileSize == wanted.size(), "ICC import missing");
            require(std::memcmp(lockHandle(reader.record.iCCprofileData, false), wanted.data(), wanted.size()) == 0,
                "ICC import changed the source profile");
            disposeHandle(reader.record.iCCprofileData); reader.record.iCCprofileData = nullptr; reader.record.iCCprofileSize = 0;
        } else require(!reader.record.iCCprofileData, "FITS unexpectedly assigned an ICC profile");
        require(reader.state == 0, "Read state leaked");
        require(!reader.xmp.empty(), "Import did not store document XMP");
        if (cfaMode != UINT32_MAX) saveDefaults({readDepth, 0, true, false, cfaMode ? 0u : 1u});
        if (useDefaults) saveDefaults({readDepth == 16 ? 32u : 16u, writeDepth, false, false});
        // Revert retains the chosen Photoshop depth, even with dialogs disabled.
        success(reader, formatSelectorReadStart);
        require(reader.record.depth == readDepth, "Revert forgot the chosen depth");
        require(reader.record.planes == planes, "Revert forgot the debayer choice");
        success(reader, formatSelectorReadFinish);
        if (useDefaults) saveDefaults({readDepth, writeDepth, false, false});

        Host writer{}; writer.entry = entry; initialize(writer, file);
        writer.xmp = reader.xmp; // The document carries XMP into Save As, independently of revertInfo.
        const auto currentProfile = profileFixture(planes, 2);
        if (format == 2) {
            writer.record.iCCprofileData = newHandle(static_cast<int32>(currentProfile.size()));
            writer.record.iCCprofileSize = static_cast<int32>(currentProfile.size());
            std::memcpy(lockHandle(writer.record.iCCprofileData, false), currentProfile.data(), currentProfile.size());
        }
        if (!useDefaults) setOptions(writer, readDepth, writeDepth);
        if (legacyOptions) {
            setOptions(writer, readDepth, 32);
            reinterpret_cast<SeizaOptions*>(lockHandle(writer.record.revertInfo, false))->version = 1;
            reinterpret_cast<TestHandle*>(writer.record.revertInfo)->bytes.resize(16);
        }
        if (interactive || useDefaults) writer.descriptor.playInfo = plugInDialogDisplay;
        const auto documentDepth = changeMode ? (readDepth == 16 ? 32u : 16u) : readDepth;
        if (changeMode && documentDepth == 16) for (auto& value : expected)
            value = static_cast<float>(std::round(std::clamp(static_cast<double>(value), 0.0, 1.0) * 32768.0) / 32768.0);
        writer.writing = true; writer.pixels = expected;
        writer.record.imageSize32.h = width; writer.record.imageSize32.v = height;
        writer.record.planes = static_cast<int16>(planes); writer.record.depth = static_cast<int16>(documentDepth);
        writer.record.imageMode = documentDepth == 16 ? (planes == 1 ? plugInModeGray16 : plugInModeRGB48) :
            (planes == 1 ? plugInModeGray32 : plugInModeRGB96);
        if (!useDefaults) success(writer, formatSelectorOptionsStart);
        success(writer, formatSelectorEstimateStart);
        success(writer, formatSelectorWritePrepare);
        success(writer, formatSelectorWriteStart);
        success(writer, formatSelectorWriteFinish);
        encoded.resize(static_cast<size_t>(io.size()));
        io.seek(0);
        require(io.read(encoded.data(), encoded.size()) == encoded.size(), "Output read failed");
        const auto outputDepth = writeDepth ? writeDepth : documentDepth;
        const std::string header(encoded.begin(), encoded.begin() + std::min<size_t>(encoded.size(), 2880));
        require(header.find(format == 1 ? (outputDepth == 16 ? "BITPIX  =                   16" : "BITPIX  =                  -32") :
            (outputDepth == 16 ? "sampleFormat=\"UInt16\"" : "sampleFormat=\"Float32\"")) != std::string::npos,
            "Saved sample type does not match the requested/document depth");
        require(header.find("metadata-document-A") != std::string::npos && header.find("EXPTIME") != std::string::npos,
            "Save As lost source metadata independently of revertInfo");
        SeizaImage* decoded = nullptr;
        require(seiza_decode(format, encoded.data(), encoded.size(), &decoded, error, sizeof(error)) == 0, error);
        SeizaImageView view{}; seiza_image_view(decoded, &view);
        auto savedExpected = expected;
        if (outputDepth == 16) for (auto& value : savedExpected)
            value = static_cast<float>(std::round(std::clamp(static_cast<double>(value), 0.0, 1.0) * 65535.0)) / 65535.0f;
        const bool equal = view.samples == expected.size() && std::equal(savedExpected.begin(), savedExpected.end(), view.pixels);
        if (format == 2) {
            std::vector<uint8_t> savedProfile;
            require(seiza_image_icc(decoded, append, &savedProfile, error, sizeof(error)) == 0, error);
            require(savedProfile == currentProfile, "Save did not use the document's converted ICC profile");
            require(handleSize(writer.record.iCCprofileData) == currentProfile.size(), "Plugin disposed host-owned profile");
        }
        seiza_image_free(decoded);
        require(equal, "Saved pixels changed across host round trip");
        if (format == 2) {
            disposeHandle(writer.record.iCCprofileData); writer.record.iCCprofileData = nullptr; writer.record.iCCprofileSize = 0;
            success(writer, formatSelectorWriteStart);
            encoded.resize(static_cast<size_t>(io.size())); io.seek(0);
            require(io.read(encoded.data(), encoded.size()) == encoded.size(), "Untagged output read failed");
            require(seiza_decode(format, encoded.data(), encoded.size(), &decoded, error, sizeof(error)) == 0, error);
            std::vector<uint8_t> removed;
            require(seiza_image_icc(decoded, append, &removed, error, sizeof(error)) == 0, error);
            seiza_image_free(decoded);
            require(removed.empty(), "Saving untagged resurrected the source ICC profile from XMP");
        }
        // A different/new document must not inherit the previous document's fields.
        writer.xmp.clear();
        success(writer, formatSelectorWriteStart);
        encoded.resize(static_cast<size_t>(io.size())); io.seek(0);
        require(io.read(encoded.data(), encoded.size()) == encoded.size(), "Isolated output read failed");
        require(std::string(encoded.begin(), encoded.end()).find("metadata-document-A") == std::string::npos,
            "Metadata leaked between documents");
        writer.record.depth = 8;
        require(call(writer, formatSelectorOptionsStart) != noErr, "8-bit save was not rejected");

        Host cancelled{}; cancelled.entry = entry; initialize(cancelled, file);
        success(cancelled, formatSelectorReadStart);
        cancelled.pixels.resize(expected.size()); cancelled.cancelAfter = 0;
        require(call(cancelled, formatSelectorReadContinue) == userCanceledErr, "Cancellation did not propagate");
        require(cancelled.state == 0 && cancelled.record.data == nullptr, "Cancellation leaked state");
        success(cancelled, formatSelectorReadFinish);
        io.seek(0);
        const char invalid[] = "not an astronomy image";
        require(io.write(invalid, sizeof(invalid)) == sizeof(invalid), "Invalid fixture write failed");
        io.finish();
        require(call(reader, formatSelectorFilterFile) == formatCannotRead, "Bad signature was accepted");
        require(call(reader, formatSelectorReadStart) != noErr && reader.state == 0, "Malformed file was accepted or leaked state");
        if (cfaMode != UINT32_MAX) saveDefaults({});
        closeFile(file);
    } catch (...) { closeFile(file); throw; }
}
static void checkDefaults() {
    auto initial = readDefaults();
    require(initial.readDepth == 32 && initial.writeDepth == 0 && initial.askOnOpen && !initial.askOnSave,
        "Missing preferences must ask on Open and match document depth on Save");
    saveDefaults({16, 32, true, false});
    auto saved = readDefaults();
    require(saved.readDepth == 16 && saved.writeDepth == 32 && saved.askOnOpen && !saved.askOnSave,
        "Preferences did not persist");
    saveDefaults({32, 16, false, true});
    saved = readDefaults();
    require(saved.readDepth == 32 && saved.writeDepth == 16 && !saved.askOnOpen && saved.askOnSave,
        "Replacing preferences did not persist");
    bool rejected = false;
    try { saveDefaults({8, 16, false, false}); } catch (const std::exception&) { rejected = true; }
    require(rejected && readDefaults().writeDepth == 16, "Invalid preferences replaced valid preferences");
    for (const char* content : {"broken", "SEIZA_DEFAULTS_V1\n16 8 0 0\n", "SEIZA_DEFAULTS_V1\n16 16 3 0\n",
        "SEIZA_DEFAULTS_V3\n16 16 0 0\n", "SEIZA_DEFAULTS_V1\n16 16 0 0\ntrailing"}) {
        { std::ofstream file(preferencesPath()); file << content; }
        const auto value = readDefaults();
        require(value.readDepth == 32 && value.writeDepth == 0 && value.askOnOpen && !value.askOnSave,
            "Malformed preferences must ask on Open and match document depth on Save");
    }
    { std::ofstream file(preferencesPath()); file << "SEIZA_DEFAULTS_V1\n16 32 1 0\n"; }
    saved = readDefaults();
    require(saved.readDepth == 16 && saved.writeDepth == 0 && saved.askOnOpen && !saved.askOnSave,
        "Legacy settings did not migrate to matching export");
    saveDefaults({32, 16, true, true});
    rememberImportChoice(16);
    saved = readDefaults();
    require(saved.readDepth == 16 && !saved.askOnOpen && saved.writeDepth == 16 && saved.askOnSave,
        "Remember import choice must disable Open prompts without changing export settings");
    rememberImportChoice(16, 5);
    require(readDefaults().debayer == 1, "Manual Bayer patterns must remember Auto, not force a pattern globally");
    rememberImportChoice(32, 0);
    require(readDefaults().debayer == 0, "Raw import choice was not remembered");
    saveDefaults({});
    require(readDefaults().writeDepth == 0, "Matching export preference did not persist");
}

int main(int argc, char** argv) {
    try {
        const bool interactive = argc == 2 && std::strcmp(argv[1], "--interactive") == 0;
        const bool debayerTest = argc == 2 && std::strcmp(argv[1], "--debayer") == 0;
        const bool rememberTest = argc == 2 && std::strcmp(argv[1], "--remember") == 0;
        const bool settings = argc == 2 && std::strcmp(argv[1], "--settings") == 0;
        // Never read or change the developer's actual settings during tests.
#ifdef _WIN32
        const auto pid = GetCurrentProcessId();
#else
        const auto pid = getpid();
#endif
        const auto testPath = std::filesystem::absolute(std::filesystem::path("build") / ("host-defaults-" + std::to_string(pid) + ".txt"));
#ifdef _WIN32
        require(SetEnvironmentVariableW(L"SEIZA_PHOTOSHOP_PREFERENCES", testPath.c_str()) != 0, "Cannot isolate preferences");
#else
        require(setenv("SEIZA_PHOTOSHOP_PREFERENCES", testPath.c_str(), 1) == 0, "Cannot isolate preferences");
#endif
        std::filesystem::remove(testPath);
        checkDefaults();
        if (interactive) saveDefaults({32, 32, true, true});
        for (uint32_t format : {1u, 2u}) {
#ifdef _WIN32
            Module module = LoadLibraryW(format == 1 ? L"dist\\SeizaFITS.8bi" : L"dist\\SeizaXISF.8bi");
#else
            Module module = dlopen(format == 1 ? "dist/macos/SeizaFITS.plugin/Contents/MacOS/SeizaFITS" :
                "dist/macos/SeizaXISF.plugin/Contents/MacOS/SeizaXISF", RTLD_NOW | RTLD_LOCAL);
            if (!module) std::fprintf(stderr, "%s\n", dlerror());
#endif
            require(module != nullptr, "Cannot load plug-in");
            if (settings) {
#ifdef _WIN32
                auto entry = reinterpret_cast<Entry>(GetProcAddress(module, "PluginMain"));
#else
                auto entry = reinterpret_cast<Entry>(dlsym(module, "PluginMain"));
#endif
                require(entry != nullptr, "PluginMain missing");
                int16 result = noErr;
                entry(formatSelectorAbout, nullptr, nullptr, &result);
                require(result == noErr, "Settings dialog failed");
                const auto saved = readDefaults();
                std::printf("Settings after %s: read=%u write=%u askOpen=%d askSave=%d\n", format == 1 ? "FITS" : "XISF",
                    saved.readDepth, saved.writeDepth, saved.askOnOpen, saved.askOnSave);
            }
            else if (debayerTest) {
                saveDefaults({});
                std::puts("Choose 16-bit and Auto debayer for the tagged RGGB fixture.");
                run(module, format, 5, 3, 1, 16, 0, true, false, false, false, 1);
            }
            else if (rememberTest) {
                saveDefaults({});
                std::puts("Choose 16-bit with Remember choice unchecked, then choose 16-bit with it checked.");
                run(module, format, 3, 2, 3, 16, 0, true);
                require(readDefaults().askOnOpen && readDefaults().readDepth == 32,
                    "Unchecked Remember choice changed global settings");
                run(module, format, 3, 2, 3, 16, 0, true);
                require(!readDefaults().askOnOpen && readDefaults().readDepth == 16,
                    "Checked Remember choice did not persist");
                run(module, format, 3, 2, 3, 16, 0, true);
                std::puts("Remember choice passed: unchecked asks again; checked persists and opens quietly.");
            }
            else if (interactive) run(module, format, 3, 2, 3, 16, 16, true);
            else for (uint32_t readDepth : {16u, 32u}) for (uint32_t writeDepth : {0u, 16u, 32u}) {
                if (writeDepth == 0) {
                    for (uint32_t cfa : {0u, 1u})
                        run(module, format, 5, 3, 1, readDepth, 0, false, false, false, false, cfa);
                    run(module, format, 5, 3, 1, readDepth, 0, false, false, false, false, 1, false);
                    run(module, format, 5, 3, 1, readDepth, 0, false, false, false, false, 2, false);
                    saveDefaults({readDepth, 0, false, false, 1});
                    run(module, format, 5, 3, 1, readDepth, 0, false, true, false, false, 1);
                    saveDefaults({});
                }
                run(module, format, 1, 1, 1, readDepth, writeDepth);
                run(module, format, 4, 1, 1, readDepth, writeDepth);
                run(module, format, 5, 1, 1, readDepth, writeDepth);
                run(module, format, 3, 2, 1, readDepth, writeDepth);
                run(module, format, 3, 2, 3, readDepth, writeDepth);
                run(module, format, 32768, 1, 1, readDepth, writeDepth);
                saveDefaults({readDepth, writeDepth, false, false});
                run(module, format, 3, 2, 3, readDepth, writeDepth, false, true);
                if (writeDepth == 0) {
                    run(module, format, 3, 2, 3, readDepth, 0, false, false, true);
                    run(module, format, 3, 2, 3, readDepth, 0, false, false, false, true);
                }
                saveDefaults({});
            }
#ifdef _WIN32
            FreeLibrary(module);
#else
            dlclose(module);
#endif
        }
        std::filesystem::remove(testPath);
        std::puts("Adobe SDK host harness passed: CFA raw/auto/manual, RGB rescaling and revert, matching document depth, mode changes, legacy migration, saved defaults, quiet import/export, skipped save options, revert, 16/32-bit pixels, rescaling, cancellation and invalid inputs.");
    } catch (const std::exception& e) { std::fprintf(stderr, "%s\n", e.what()); return 1; }
}
