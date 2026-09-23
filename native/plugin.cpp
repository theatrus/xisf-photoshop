// Photoshop file-format adapter. Compile against Adobe's real SDK:
// FormatRecord is deliberately never re-declared or mirrored in Rust.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#ifdef _WIN32
#include <windows.h>
#else
#include <CoreFoundation/CoreFoundation.h>
#include <CoreServices/CoreServices.h>
#endif
#include "PIDefines.h"
#include "PIFormat.h"
#include "PIProperties.h"
#include "seiza_codec.h"
#include "host_file.h"
#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>
#include <memory>
#include <stdexcept>
#include <sstream>
#include <vector>
#include "options_ui.h"

#ifndef SEIZA_FORMAT
#error SEIZA_FORMAT must be 1 (FITS) or 2 (XISF)
#endif

namespace {
constexpr uint32_t kFormat = SEIZA_FORMAT;
constexpr size_t kIoChunk = 1024 * 1024;
struct HostError { int16 code; };
struct State {
    std::unique_ptr<SeizaImage, decltype(&seiza_image_free)> image{nullptr, seiza_image_free};
    SeizaImageView view{};
    uint32_t depth = 32;
    std::vector<uint16_t> integerRow;
    double minimum = 0;
    double maximum = 1;
};

bool canStoreOptions(const FormatRecord& r) {
    const auto* h = r.handleProcs;
    return h && h->handleProcsVersion >= 1 && h->numHandleProcs >= 6 &&
        h->newProc && h->disposeProc && h->getSizeProc && h->lockProc && h->unlockProc;
}

struct OwnedHandle {
    Handle value;
    HandleProcs* procs;
    ~OwnedHandle() { if (value) procs->disposeProc(value); }
};

void requireMetadataSuite(const FormatRecord& r) {
    if (!canStoreOptions(r) || !r.propertyProcs || r.propertyProcs->numPropertyProcs < 2 ||
        !r.propertyProcs->getPropertyProc || !r.propertyProcs->setPropertyProc)
        throw std::runtime_error("Photoshop's property suite is required to preserve astronomy metadata");
}

int32_t appendMetadata(void* context, const uint8_t* bytes, size_t count) noexcept {
    try {
        auto& data = *static_cast<std::vector<uint8_t>*>(context);
        data.insert(data.end(), bytes, bytes + count);
        return 0;
    } catch (...) { return 1; }
}

void storeMetadata(FormatRecord& r, const SeizaImage* image) {
    if (r.openForPreview) return;
    requireMetadataSuite(r);
    std::vector<uint8_t> bytes;
    char error[1024]{};
    if (seiza_image_xmp(image, appendMetadata, &bytes, error, sizeof(error))) throw std::runtime_error(error);
    if (bytes.size() > INT32_MAX) throw std::runtime_error("Astronomy metadata is too large");
    OwnedHandle handle{r.handleProcs->newProc(static_cast<int32>(bytes.size())), r.handleProcs};
    if (!handle.value) throw std::bad_alloc();
    auto* data = r.handleProcs->lockProc(handle.value, false);
    if (!data) throw std::bad_alloc();
    std::memcpy(data, bytes.data(), bytes.size());
    r.handleProcs->unlockProc(handle.value);
    const auto result = r.propertyProcs->setPropertyProc(kPhotoshopSignature, propXMP, 0, 0, handle.value);
    if (result) throw std::runtime_error("Photoshop could not retain astronomy metadata on the document");
}

std::vector<uint8_t> loadMetadata(FormatRecord& r) {
    requireMetadataSuite(r);
    OwnedHandle handle{nullptr, r.handleProcs};
    const auto result = r.propertyProcs->getPropertyProc(kPhotoshopSignature, propXMP, 0, nullptr, &handle.value);
    if (result) throw std::runtime_error("Photoshop could not read document metadata");
    if (!handle.value) return {};
    const auto size = r.handleProcs->getSizeProc(handle.value);
    if (size < 0 || size > 128 * 1024 * 1024) throw std::runtime_error("Document metadata exceeds the size limit");
    if (!size) return {};
    auto* data = r.handleProcs->lockProc(handle.value, false);
    if (!data) throw std::bad_alloc();
    // Unlock before any allocation that could throw.
    std::vector<uint8_t> bytes;
    try { bytes.assign(reinterpret_cast<uint8_t*>(data), reinterpret_cast<uint8_t*>(data) + size); }
    catch (...) { r.handleProcs->unlockProc(handle.value); throw; }
    r.handleProcs->unlockProc(handle.value);
    return bytes;
}

void storeProfile(FormatRecord& r, const SeizaImage* image) {
    if (kFormat != 2 || !r.canUseICCProfiles) return;
    std::vector<uint8_t> bytes;
    char error[1024]{};
    if (seiza_image_icc(image, appendMetadata, &bytes, error, sizeof(error))) throw std::runtime_error(error);
    if (bytes.empty()) return;
    if (!canStoreOptions(r)) throw std::runtime_error("Photoshop's handle suite is required for ICC profiles");
    OwnedHandle handle{r.handleProcs->newProc(static_cast<int32>(bytes.size())), r.handleProcs};
    if (!handle.value) throw std::bad_alloc();
    auto* data = r.handleProcs->lockProc(handle.value, false);
    if (!data) throw std::bad_alloc();
    std::memcpy(data, bytes.data(), bytes.size());
    r.handleProcs->unlockProc(handle.value);
    // PIFormat.h: Photoshop consumes this after ReadFinish and owns disposal.
    r.iCCprofileData = handle.value;
    r.iCCprofileSize = static_cast<int32>(bytes.size());
    handle.value = nullptr;
}

std::vector<uint8_t> loadProfile(FormatRecord& r) {
    if (kFormat != 2 || !r.canUseICCProfiles) return {};
    // A null/empty host profile means untagged or "Embed Color Profile" disabled.
    if (r.iCCprofileSize == 0) return {};
    if (r.iCCprofileSize < 0 || r.iCCprofileSize > 16 * 1024 * 1024 || !r.iCCprofileData || !canStoreOptions(r))
        throw std::runtime_error("Invalid Photoshop ICC profile buffer");
    if (r.handleProcs->getSizeProc(r.iCCprofileData) < r.iCCprofileSize)
        throw std::runtime_error("Truncated Photoshop ICC profile handle");
    std::vector<uint8_t> bytes(static_cast<size_t>(r.iCCprofileSize));
    auto* data = r.handleProcs->lockProc(r.iCCprofileData, false);
    if (!data) throw std::bad_alloc();
    std::memcpy(bytes.data(), data, bytes.size());
    r.handleProcs->unlockProc(r.iCCprofileData);
    // The host owns this handle on writes too; never dispose it here.
    return bytes;
}

bool loadOptions(FormatRecord& r, SeizaOptions& options) {
    if (!r.revertInfo || !canStoreOptions(r)) return false;
    const auto size = r.handleProcs->getSizeProc(r.revertInfo);
    if (size != 16 && size != sizeof(SeizaOptions)) return false;
    auto* data = r.handleProcs->lockProc(r.revertInfo, false);
    if (!data) throw std::bad_alloc();
    SeizaOptions stored;
    std::memcpy(&stored, data, static_cast<size_t>(size));
    r.handleProcs->unlockProc(r.revertInfo);
    if (stored.magic != options.magic || stored.version < 1 || stored.version > options.version ||
        (stored.version == 3 && size != sizeof(stored)) || stored.debayer > 5 ||
        (stored.readDepth != 16 && stored.readDepth != 32) ||
        (stored.writeDepth != 16 && stored.writeDepth != 32 &&
            !(stored.version >= 2 && stored.writeDepth == 0))) return false;
    if (stored.version == 1) {
        stored.writeDepth = 0;
    }
    if (stored.version < 3) stored.debayer = 0;
    stored.version = options.version;
    options = stored;
    return true;
}

void storeOptions(FormatRecord& r, const SeizaOptions& options) {
    if (!canStoreOptions(r)) {
        if (options.readDepth == 16 || options.writeDepth == 16 || options.debayer)
            throw std::runtime_error("Photoshop's handle suite is required to remember conversion options");
        return;
    }
    auto& h = *r.handleProcs;
    Handle handle = h.newProc(sizeof(options));
    if (!handle) throw std::bad_alloc();
    auto* data = h.lockProc(handle, false);
    if (!data) { h.disposeProc(handle); throw std::bad_alloc(); }
    std::memcpy(data, &options, sizeof(options));
    h.unlockProc(handle);
    if (r.revertInfo) h.disposeProc(r.revertInfo);
    r.revertInfo = handle;
}

bool silent(const FormatRecord& r) {
    return r.openForPreview || (r.descriptorParameters &&
        r.descriptorParameters->playInfo == plugInDialogSilent);
}

uint32_t importDepth(FormatRecord& r, State& state) {
    const auto& view = state.view;
    const auto defaults = readDefaults();
    SeizaOptions options;
    options.readDepth = defaults.readDepth;
    options.writeDepth = defaults.writeDepth;
    options.debayer = defaults.debayer;
    const bool reverting = loadOptions(r, options);
    const bool singleChannel = view.planes == 1;
    state.minimum = state.maximum = view.pixels[0];
    for (size_t i = 0; i < view.samples; ++i) {
        if (i % kIoChunk == 0 && r.abortProc && r.abortProc()) throw HostError{userCanceledErr};
        const double v = view.pixels[i];
        state.minimum = std::min(state.minimum, v);
        state.maximum = std::max(state.maximum, v);
    }
    bool remember = false;
    if (!reverting && defaults.askOnOpen && !silent(r)) {
        std::ostringstream text;
        text << "Choose the Photoshop document depth.\n\n"
             << "32-bit float preserves the decoded values, including negative and HDR values. "
             << "Float32 has up to 24 bits of significant precision.\n\n"
             << "16-bit integer enables Photoshop's normal 16-bit tools, but its internal range is "
             << "0..32768 (about 15 bits plus an endpoint). The image minimum and maximum are rescaled "
             << "to this range, using one shared scale for all channels. No samples are clipped. "
             << "Rounding loses precision, and the original absolute scale is not retained on save.\n\n"
             << "Source range: " << state.minimum << " to " << state.maximum << ".";
        if (state.minimum == state.maximum) text << " This constant image will become zero (black).";
        if (singleChannel) {
            static const char* patterns[] = {"No Bayer metadata", "RGGB", "BGGR", "GRBG", "GBRG", "Unsupported Bayer metadata"};
            text << "\n\nCFA: " << patterns[std::min(view.cfa_pattern, 5u)] << ". Offsets: "
                 << view.cfa_x_offset << ", " << view.cfa_y_offset << ". Debayer uses bilinear interpolation before rescaling.";
            text << " Remember choice saves Raw or Auto globally; manual patterns stay with this document.";
            if (view.cfa_invalid_offsets) text << " Invalid offsets: open raw or correct the source headers.";
        }
        options.readDepth = chooseDepth("FITS / XISF - Open image", text.str(), options.readDepth, &remember,
            singleChannel ? &options.debayer : nullptr);
        if (!options.readDepth) throw HostError{userCanceledErr};
    }
    char error[1024]{};
    if (seiza_image_debayer(state.image.get(), options.debayer, error, sizeof(error))) throw std::runtime_error(error);
    if (seiza_image_view(state.image.get(), &state.view)) throw std::runtime_error("Invalid debayered image");
    // The shared RGB range must be measured after interpolation, before quantizing.
    state.minimum = state.maximum = view.pixels[0];
    for (size_t i = 0; i < view.samples; ++i) {
        if (i % kIoChunk == 0 && r.abortProc && r.abortProc()) throw HostError{userCanceledErr};
        state.minimum = std::min(state.minimum, static_cast<double>(view.pixels[i]));
        state.maximum = std::max(state.maximum, static_cast<double>(view.pixels[i]));
    }
    if (!r.openForPreview) {
        storeOptions(r, options);
        if (remember) rememberImportChoice(options.readDepth, singleChannel ? options.debayer : UINT32_MAX);
    }
    return r.openForPreview ? 32 : options.readDepth;
}

void writeOptions(FormatRecord& r) {
    const auto defaults = readDefaults();
    SeizaOptions options;
    options.readDepth = defaults.readDepth;
    options.writeDepth = defaults.writeDepth;
    loadOptions(r, options);
    if (defaults.askOnSave && !silent(r)) {
        const std::string text =
            "Choose the sample format stored in the FITS/XISF file.\n\n"
            "32-bit float preserves the current document's sample values. Saving a 16-bit document "
            "as Float32 cannot recover precision already lost on import.\n\n"
            "16-bit integer rounds normalized 0..1 values to 0..65535. Negative values become 0 "
            "and values above 1 become 65535. Float32's 24-bit significant precision and HDR range "
            "are lost. No stretch or automatic rescaling is applied.\n\n"
            "Photoshop 16-bit documents already have about 15 bits plus an endpoint of precision. "
            "Choose 32-bit float to avoid further quantization.";
        options.writeDepth = chooseDepth("FITS / XISF - Save image", text,
            options.writeDepth ? options.writeDepth : static_cast<uint32_t>(r.depth));
        if (!options.writeDepth) throw HostError{userCanceledErr};
    }
    storeOptions(r, options);
    r.data = nullptr;
}

void checkCancel(FormatRecord& r) {
    if (r.abortProc && r.abortProc()) throw HostError{userCanceledErr};
}

std::vector<uint8_t> readFile(FormatRecord& r) {
    HostFile file(r);
    const auto length = file.size();
    if (length <= 0 || static_cast<uint64_t>(length) > std::numeric_limits<size_t>::max())
        throw std::runtime_error("Invalid astronomy file size");
    file.seek(0);
    std::vector<uint8_t> bytes(static_cast<size_t>(length));
    size_t offset = 0;
    while (offset < bytes.size()) {
        checkCancel(r);
        const auto count = std::min(kIoChunk, bytes.size() - offset);
        const auto received = file.read(bytes.data() + offset, count);
        if (received == 0)
            throw std::runtime_error("Cannot read astronomy file");
        offset += received;
    }
    return bytes;
}

void filterFile(FormatRecord& r) {
    HostFile file(r);
    const auto saved = file.position();
    file.seek(0);
    char bytes[9]{};
    size_t received = 0;
    try {
        while (received < sizeof(bytes)) {
            const auto count = file.read(bytes + received, sizeof(bytes) - received);
            if (!count) break;
            received += count;
        }
    } catch (...) { file.seek(saved); throw; }
    file.seek(saved);
    const bool signature = kFormat == 1 ? received >= 9 && std::memcmp(bytes, "SIMPLE  =", 9) == 0
                                       : received >= 8 && std::memcmp(bytes, "XISF0100", 8) == 0;
    if (!signature) throw HostError{formatCannotRead};
}

void coordinates(FormatRecord& r, uint32_t width, uint32_t height) {
    if (!r.HostSupports32BitCoordinates && (width > 32767 || height > 32767))
        throw std::runtime_error("This Photoshop host does not support large image coordinates");
    r.PluginUsing32BitCoordinates = r.HostSupports32BitCoordinates;
    r.imageSize32.h = static_cast<int32>(width);
    r.imageSize32.v = static_cast<int32>(height);
    r.imageSize.h = static_cast<int16>(std::min(width, 32767u));
    r.imageSize.v = static_cast<int16>(std::min(height, 32767u));
}

void row(FormatRecord& r, uint32_t width, uint32_t y, int16 plane, void* pixels) {
    r.theRect32.left = 0;
    r.theRect32.right = static_cast<int32>(width);
    r.theRect32.top = static_cast<int32>(y);
    r.theRect32.bottom = static_cast<int32>(y + 1);
    if (!r.PluginUsing32BitCoordinates) {
        r.theRect.left = 0;
        r.theRect.right = static_cast<int16>(width);
        r.theRect.top = static_cast<int16>(y);
        r.theRect.bottom = static_cast<int16>(y + 1);
    }
    r.loPlane = r.hiPlane = plane;
    r.colBytes = r.depth == 16 ? sizeof(uint16_t) : sizeof(float);
    r.rowBytes = static_cast<int32>(width * r.colBytes);
    r.planeBytes = 0; // one plane per transfer
    r.data = pixels;
}

void advance(FormatRecord& r) {
    checkCancel(r);
    if (!r.advanceState) throw std::runtime_error("Photoshop advanceState is unavailable");
    const auto status = r.advanceState();
    if (status) throw HostError{static_cast<int16>(status)};
}

void readStart(FormatRecord& r, intptr_t& persistent) {
    if (persistent) throw std::runtime_error("Image read already in progress");
    auto state = std::make_unique<State>();
    auto bytes = readFile(r);
    char error[1024]{};
    SeizaImage* decoded = nullptr;
    if (seiza_decode(kFormat, bytes.data(), bytes.size(), &decoded, error, sizeof(error)))
        throw std::runtime_error(error);
    state->image.reset(decoded);
    if (seiza_image_view(decoded, &state->view)) throw std::runtime_error("Invalid decoded image");
    coordinates(r, state->view.width, state->view.height);
    state->depth = importDepth(r, *state);
    r.imageMode = state->view.planes == 1 ? plugInModeGrayScale : plugInModeRGBColor;
    r.depth = static_cast<int16>(state->depth);
    if (r.depth == 16) {
        r.maxValue = 32768;
        state->integerRow.resize(state->view.width);
    }
    r.planes = static_cast<int16>(state->view.planes);
    for (int16 p = 0; p < r.planes; ++p) r.planeMap[p] = p;
    r.transparencyPlane = 0;
    r.transparencyMatting = 0;
    r.imageHRes = r.imageVRes = 72 << 16;
    r.theRect = {};
    r.theRect32 = {};
    // Empty rectangle schedules Continue without transferring pixels yet.
    r.data = const_cast<float*>(state->view.pixels);
    persistent = reinterpret_cast<intptr_t>(state.release());
}

void readContinue(FormatRecord& r, intptr_t persistent) {
    auto* state = reinterpret_cast<State*>(persistent);
    if (!state) throw std::runtime_error("Missing image read state");
    const auto& v = state->view;
    for (uint32_t p = 0; p < v.planes; ++p) {
        for (uint32_t y = 0; y < v.height; ++y) {
            auto* samples = v.pixels + (static_cast<size_t>(p) * v.height + y) * v.width;
            if (state->depth == 16) {
                // Use double for the range so even opposite extreme finite f32
                // endpoints rescale without overflowing. One range for all planes.
                const double range = state->maximum - state->minimum;
                for (uint32_t x = 0; x < v.width; ++x)
                    state->integerRow[x] = range == 0 ? 0 : static_cast<uint16_t>(std::lround(
                        ((static_cast<double>(samples[x]) - state->minimum) / range) * 32768.0));
                row(r, v.width, y, static_cast<int16>(p), state->integerRow.data());
            } else row(r, v.width, y, static_cast<int16>(p), const_cast<float*>(samples));
            advance(r);
            if (r.progressProc) r.progressProc(static_cast<int32>(p * v.height + y + 1), static_cast<int32>(v.planes * v.height));
        }
    }
    r.data = nullptr;
    storeMetadata(r, state->image.get());
    storeProfile(r, state->image.get());
}

void validateWrite(FormatRecord& r) {
    const bool mono = r.imageMode == plugInModeGrayScale || r.imageMode == plugInModeGray16 || r.imageMode == plugInModeGray32;
    const bool rgb = r.imageMode == plugInModeRGBColor || r.imageMode == plugInModeRGB48 || r.imageMode == plugInModeRGB96;
    if ((r.depth != 16 && r.depth != 32) || (!mono && !rgb) || r.planes != (mono ? 1 : 3))
        throw std::runtime_error("Save requires 16-bit or 32-bit grayscale or RGB without alpha channels");
}

struct WriteContext { FormatRecord* record; bool cancelled = false; };
int32_t writeBytes(void* opaque, const uint8_t* bytes, size_t length) noexcept {
    auto& context = *static_cast<WriteContext*>(opaque);
    try {
        auto& r = *context.record;
        HostFile file(r);
        while (length) {
            if (r.abortProc && r.abortProc()) { context.cancelled = true; return 1; }
            const auto count = std::min(kIoChunk, length);
            const auto written = file.write(bytes, count);
            if (written == 0) return 1;
            bytes += written;
            length -= written;
        }
        return 0;
    } catch (...) { return 1; }
}

void writeStart(FormatRecord& r) {
    validateWrite(r);
    const auto metadata = loadMetadata(r);
    const auto profile = loadProfile(r);
    SeizaOptions options;
    if (!loadOptions(r, options)) {
        writeOptions(r); // Some hosts skip the options sequence entirely.
        if (!loadOptions(r, options)) options.writeDepth = readDefaults().writeDepth;
    }
    for (int16 p = 0; p < r.planes; ++p) r.planeMap[p] = p;
    const auto w = r.HostSupports32BitCoordinates ? r.imageSize32.h : r.imageSize.h;
    const auto h = r.HostSupports32BitCoordinates ? r.imageSize32.v : r.imageSize.v;
    if (w <= 0 || h <= 0 || w > 300000 || h > 300000) throw std::runtime_error("Invalid image dimensions");
    coordinates(r, w, h);
    const size_t count = static_cast<size_t>(w) * h * r.planes;
    if (count > std::numeric_limits<size_t>::max() / sizeof(float)) throw std::bad_alloc();
    std::vector<float> samples(count);
    std::vector<uint16_t> integerRow(r.depth == 16 ? w : 0);
    for (int16 p = 0; p < r.planes; ++p) {
        for (int32 y = 0; y < h; ++y) {
            auto* destination = samples.data() + (static_cast<size_t>(p) * h + y) * w;
            row(r, w, y, p, r.depth == 16 ? static_cast<void*>(integerRow.data()) : destination);
            advance(r);
            if (r.depth == 16) {
                for (int32 x = 0; x < w; ++x) {
                    if (integerRow[x] > 32768) throw std::runtime_error("Photoshop supplied a 16-bit sample outside 0..32768");
                    destination[x] = static_cast<float>(integerRow[x]) / 32768.0f;
                }
            }
            if (r.progressProc) r.progressProc(p * h + y + 1, r.planes * h);
        }
    }
    r.data = nullptr;
    HostFile file(r);
    file.seek(0);
    WriteContext context{&r};
    char error[1024]{};
    const auto outputDepth = options.writeDepth ? options.writeDepth : static_cast<uint32_t>(r.depth);
    if (seiza_encode_with_profile(kFormat, outputDepth, w, h, r.planes, samples.data(), count,
        metadata.data(), metadata.size(), kFormat == 2 && r.canUseICCProfiles ? 1u : 0u,
        profile.data(), profile.size(), writeBytes, &context, error, sizeof(error))) {
        if (context.cancelled) throw HostError{userCanceledErr};
        throw std::runtime_error(error);
    }
    file.finish();
}

void report(FormatRecord& r, int16& result, const char* message) noexcept {
    r.data = nullptr;
    if (r.errorString) {
        const auto length = std::min<size_t>(std::strlen(message), 255);
        (*r.errorString)[0] = static_cast<unsigned char>(length);
        std::memcpy(*r.errorString + 1, message, length);
        result = errReportString;
    } else result = formatBadParameters;
}

void cleanup(FormatRecord& r, intptr_t& persistent) noexcept {
    r.data = nullptr;
    delete reinterpret_cast<State*>(persistent);
    persistent = 0;
}
} // namespace

#ifdef _WIN32
#define SEIZA_EXPORT extern "C" __declspec(dllexport)
#else
#define SEIZA_EXPORT extern "C" __attribute__((visibility("default")))
#endif
SEIZA_EXPORT void MACPASCAL PluginMain(
    const int16 selector, FormatRecord* record, intptr_t* persistent, int16* result) noexcept {
    if (!result) return;
    *result = noErr;
    if (selector == formatSelectorAbout) {
        // About uses AboutRecord, not FormatRecord. Do not dereference record.
        try {
            auto defaults = readDefaults();
            if (editDefaults(defaults)) saveDefaults(defaults);
        } catch (...) {
            *result = formatBadParameters;
#ifdef _WIN32
            MessageBoxW(GetActiveWindow(), L"Could not save Seiza defaults. Check that your application preferences folder is writable.",
                L"Seiza settings", MB_OK | MB_ICONERROR);
#else
            CFUserNotificationDisplayNotice(0, kCFUserNotificationStopAlertLevel, nullptr, nullptr, nullptr,
                CFSTR("Seiza settings"), CFSTR("Could not save Seiza defaults. Check that your application preferences folder is writable."), CFSTR("OK"));
#endif
        }
        return;
    }
    if (!record || !persistent) { *result = formatBadParameters; return; }
    auto& r = *record;
    r.PluginUsing32BitCoordinates = r.HostSupports32BitCoordinates;
#ifndef _WIN32
    r.pluginUsingPOSIXIO = r.hostSupportsPOSIXIO;
#endif
    try {
        switch (selector) {
        case formatSelectorReadPrepare:
        case formatSelectorWritePrepare:
        case formatSelectorOptionsPrepare:
        case formatSelectorEstimatePrepare: r.maxData = 0; break;
        case formatSelectorFilterFile: filterFile(r); break;
        case formatSelectorReadStart: readStart(r, *persistent); break;
        case formatSelectorReadContinue: readContinue(r, *persistent); break;
        case formatSelectorReadFinish: cleanup(r, *persistent); break;
        case formatSelectorOptionsStart: validateWrite(r); writeOptions(r); break;
        case formatSelectorEstimateStart: {
            validateWrite(r);
            const auto w = r.HostSupports32BitCoordinates ? r.imageSize32.h : r.imageSize.h;
            const auto h = r.HostSupports32BitCoordinates ? r.imageSize32.v : r.imageSize.v;
            const auto bytes = static_cast<int64_t>(w) * h * r.planes * 4 + 65536 +
                static_cast<int64_t>(loadMetadata(r).size()) + static_cast<int64_t>(loadProfile(r).size());
            r.minDataBytes = r.maxDataBytes = static_cast<int32>(std::min<int64_t>(bytes, INT32_MAX));
            r.data = nullptr;
            break;
        }
        case formatSelectorWriteStart: writeStart(r); break;
        case formatSelectorWriteContinue:
        case formatSelectorWriteFinish:
        case formatSelectorOptionsContinue:
        case formatSelectorOptionsFinish:
        case formatSelectorEstimateContinue:
        case formatSelectorEstimateFinish: r.data = nullptr; break;
        default: *result = formatBadParameters; break;
        }
    } catch (const HostError& e) { r.data = nullptr; *result = e.code; }
      catch (const std::bad_alloc&) { r.data = nullptr; *result = memFullErr; }
      catch (const std::exception& e) { report(r, *result, e.what()); }
      catch (...) { report(r, *result, "Unexpected astronomy plug-in error"); }
    // Finish can still be sent by Photoshop after an error; cleanup is idempotent.
    if (*result != noErr && *persistent) cleanup(r, *persistent);
}
