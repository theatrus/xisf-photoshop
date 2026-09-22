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
#include "seiza_codec.h"
#include "host_file.h"
#include <algorithm>
#include <cstdio>
#include <cstring>
#include <stdexcept>
#include <string>
#include <vector>

using Entry = void (*)(int16, FormatRecord*, intptr_t*, int16*);
struct Host {
    FormatRecord record{};
    intptr_t state = 0;
    Str255 error{};
    Entry entry;
    std::vector<float> pixels;
    bool writing = false;
    int transfers = 0;
    int cancelAfter = -1;
};
static Host* current = nullptr;
static void require(bool value, const char* message) {
    if (!value) throw std::runtime_error(message);
}
static Boolean MACPASCAL abortProc() {
    return current->cancelAfter >= 0 && current->transfers >= current->cancelAfter;
}
static OSErr MACPASCAL advanceProc() {
    auto& h = *current;
    auto& r = h.record;
    require(r.depth == 32 && r.data && r.colBytes == 4, "Invalid host pixel transfer");
    require(r.theRect32.left == 0 && r.theRect32.right == r.imageSize32.h &&
        r.theRect32.bottom == r.theRect32.top + 1 && r.loPlane == r.hiPlane, "Invalid rectangle/plane");
    const size_t offset = (static_cast<size_t>(r.loPlane) * r.imageSize32.v + r.theRect32.top) * r.imageSize32.h;
    require(offset + r.imageSize32.h <= h.pixels.size(), "Transfer outside image");
    auto* samples = static_cast<float*>(r.data);
    if (h.writing) std::copy_n(h.pixels.data() + offset, r.imageSize32.h, samples);
    else std::copy_n(samples, r.imageSize32.h, h.pixels.data() + offset);
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
}
static void closeFile(File file) {
#ifdef _WIN32
    CloseHandle(file);
#else
    close(file);
#endif
}
static void run(Module module, uint32_t format, uint32_t width, uint32_t height, uint32_t planes) {
#ifdef _WIN32
    auto entry = reinterpret_cast<Entry>(GetProcAddress(module, "PluginMain"));
    require(FindResourceW(module, MAKEINTRESOURCEW(16000), L"PiPL") != nullptr, "PiPL resource missing");
#else
    auto entry = reinterpret_cast<Entry>(dlsym(module, "PluginMain"));
#endif
    require(entry != nullptr, "PluginMain export is missing");
    std::vector<float> expected(static_cast<size_t>(width) * height * planes);
    for (size_t i = 0; i < expected.size(); ++i) expected[i] = static_cast<float>(i % 37) / 8.0f - 0.25f;
    std::vector<uint8_t> encoded;
    char error[1024]{};
    require(seiza_encode(format, width, height, planes, expected.data(), expected.size(), append, &encoded, error, sizeof(error)) == 0, error);
    // A Unicode path also verifies that plug-in I/O only uses host file handles.
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
        HostFile io(reader.record);
        require(io.write(encoded.data(), encoded.size()) == encoded.size(), "Fixture write failed");
        io.seek(7);
        success(reader, formatSelectorFilterFile);
        require(io.position() == 7, "Filter did not preserve file position");
        success(reader, formatSelectorReadPrepare);
        success(reader, formatSelectorReadStart);
        require(reader.record.imageSize32.h == width && reader.record.imageSize32.v == height &&
            reader.record.planes == planes && reader.record.depth == 32, "Incorrect image description");
        reader.pixels.resize(expected.size());
        success(reader, formatSelectorReadContinue);
        require(reader.pixels == expected && reader.record.data == nullptr, "Host received incorrect pixels");
        success(reader, formatSelectorReadFinish);
        require(reader.state == 0, "Read state leaked");

        Host writer{}; writer.entry = entry; initialize(writer, file);
        writer.writing = true; writer.pixels = expected;
        writer.record.imageSize32.h = width; writer.record.imageSize32.v = height;
        writer.record.planes = static_cast<int16>(planes); writer.record.depth = 32;
        writer.record.imageMode = planes == 1 ? plugInModeGrayScale : plugInModeRGBColor;
        success(writer, formatSelectorOptionsStart);
        success(writer, formatSelectorEstimateStart);
        success(writer, formatSelectorWritePrepare);
        success(writer, formatSelectorWriteStart);
        success(writer, formatSelectorWriteFinish);
        encoded.resize(static_cast<size_t>(io.size()));
        io.seek(0);
        require(io.read(encoded.data(), encoded.size()) == encoded.size(), "Output read failed");
        SeizaImage* decoded = nullptr;
        require(seiza_decode(format, encoded.data(), encoded.size(), &decoded, error, sizeof(error)) == 0, error);
        SeizaImageView view{}; seiza_image_view(decoded, &view);
        const bool equal = view.samples == expected.size() && std::equal(expected.begin(), expected.end(), view.pixels);
        seiza_image_free(decoded);
        require(equal, "Saved pixels changed across host round trip");
        writer.record.depth = 16;
        require(call(writer, formatSelectorOptionsStart) != noErr, "16-bit save was not rejected");

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
        closeFile(file);
    } catch (...) { closeFile(file); throw; }
}
int main() {
    try {
        for (uint32_t format : {1u, 2u}) {
#ifdef _WIN32
            Module module = LoadLibraryW(format == 1 ? L"dist\\SeizaFITS.8bi" : L"dist\\SeizaXISF.8bi");
#else
            Module module = dlopen(format == 1 ? "dist/macos/SeizaFITS.plugin/Contents/MacOS/SeizaFITS" :
                "dist/macos/SeizaXISF.plugin/Contents/MacOS/SeizaXISF", RTLD_NOW | RTLD_LOCAL);
            if (!module) std::fprintf(stderr, "%s\n", dlerror());
#endif
            require(module != nullptr, "Cannot load plug-in");
            run(module, format, 3, 2, 1);
            run(module, format, 3, 2, 3);
            run(module, format, 32768, 1, 1);
#ifdef _WIN32
            FreeLibrary(module);
#else
            dlclose(module);
#endif
        }
        std::puts("Adobe SDK host harness: both plug-ins passed mono/RGB round trips, large coordinates, cancellation, invalid modes and malformed input.");
    } catch (const std::exception& e) { std::fprintf(stderr, "%s\n", e.what()); return 1; }
}
