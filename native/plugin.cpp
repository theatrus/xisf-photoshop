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
#include "seiza_codec.h"
#include "host_file.h"
#include <algorithm>
#include <cstring>
#include <limits>
#include <memory>
#include <stdexcept>
#include <vector>

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
};

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
    r.colBytes = sizeof(float);
    r.rowBytes = static_cast<int32>(width * sizeof(float));
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
    r.imageMode = state->view.planes == 1 ? plugInModeGrayScale : plugInModeRGBColor;
    r.depth = 32;
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
            row(r, v.width, y, static_cast<int16>(p), const_cast<float*>(samples));
            advance(r);
            if (r.progressProc) r.progressProc(static_cast<int32>(p * v.height + y + 1), static_cast<int32>(v.planes * v.height));
        }
    }
    r.data = nullptr;
}

void validateWrite(FormatRecord& r) {
    const bool mono = r.imageMode == plugInModeGrayScale || r.imageMode == plugInModeGray32;
    const bool rgb = r.imageMode == plugInModeRGBColor || r.imageMode == plugInModeRGB96;
    if (r.depth != 32 || (!mono && !rgb) || r.planes != (mono ? 1 : 3))
        throw std::runtime_error("Save requires 32-bit grayscale or RGB without alpha channels");
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
    for (int16 p = 0; p < r.planes; ++p) r.planeMap[p] = p;
    const auto w = r.HostSupports32BitCoordinates ? r.imageSize32.h : r.imageSize.h;
    const auto h = r.HostSupports32BitCoordinates ? r.imageSize32.v : r.imageSize.v;
    if (w <= 0 || h <= 0 || w > 300000 || h > 300000) throw std::runtime_error("Invalid image dimensions");
    coordinates(r, w, h);
    const size_t count = static_cast<size_t>(w) * h * r.planes;
    if (count > std::numeric_limits<size_t>::max() / sizeof(float)) throw std::bad_alloc();
    std::vector<float> samples(count);
    for (int16 p = 0; p < r.planes; ++p) {
        for (int32 y = 0; y < h; ++y) {
            row(r, w, y, p, samples.data() + (static_cast<size_t>(p) * h + y) * w);
            advance(r);
            if (r.progressProc) r.progressProc(p * h + y + 1, r.planes * h);
        }
    }
    r.data = nullptr;
    HostFile file(r);
    file.seek(0);
    WriteContext context{&r};
    char error[1024]{};
    if (seiza_encode(kFormat, w, h, r.planes, samples.data(), count, writeBytes, &context, error, sizeof(error))) {
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
#ifdef _WIN32
        MessageBoxW(nullptr, L"FITS and XISF file support powered by seiza-fits and seiza-xisf.\nVersion 0.1.0", L"Seiza Astronomy Formats", MB_OK);
#else
        CFUserNotificationDisplayNotice(0, kCFUserNotificationNoteAlertLevel, nullptr, nullptr, nullptr,
            CFSTR("Seiza Astronomy Formats"), CFSTR("FITS and XISF support powered by seiza-fits and seiza-xisf. Version 0.1.0"), CFSTR("OK"));
#endif
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
        case formatSelectorOptionsStart: validateWrite(r); r.data = nullptr; break;
        case formatSelectorEstimateStart: {
            validateWrite(r);
            const auto w = r.HostSupports32BitCoordinates ? r.imageSize32.h : r.imageSize.h;
            const auto h = r.HostSupports32BitCoordinates ? r.imageSize32.v : r.imageSize.v;
            const auto bytes = static_cast<int64_t>(w) * h * r.planes * 4 + 65536;
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
