#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif
typedef struct SeizaImage SeizaImage;
typedef struct SeizaImageView {
    uint32_t width, height, planes;
    size_t samples;
    const float* pixels;
} SeizaImageView;
// Format: 1=FITS, 2=XISF. Status: 0=success, 1=error. Errors are NUL-terminated.
// Decode owns a copy of the pixels; input bytes can be released on return.
int32_t seiza_decode(uint32_t format, const uint8_t* bytes, size_t length,
    SeizaImage** output, char* error, size_t capacity);
// The borrowed view is valid until seiza_image_free. Never free pixels directly.
int32_t seiza_image_view(const SeizaImage* image, SeizaImageView* view);
void seiza_image_free(SeizaImage* image);
// Callback must consume all bytes or return nonzero; it must not throw.
typedef int32_t (*SeizaWriteCallback)(void*, const uint8_t*, size_t);
int32_t seiza_encode(uint32_t format, uint32_t width, uint32_t height, uint32_t planes,
    const float* pixels, size_t samples, SeizaWriteCallback callback, void* context,
    char* error, size_t capacity);
// depth: 32=Float32 or 16=UInt16. UInt16 clips to [0,1] and rounds to [0,65535].
// Caller must obtain consent to lose precision/range before selecting depth 16.
int32_t seiza_encode_depth(uint32_t format, uint32_t depth, uint32_t width, uint32_t height,
    uint32_t planes, const float* pixels, size_t samples, SeizaWriteCallback callback,
    void* context, char* error, size_t capacity);
#ifdef __cplusplus
}
#endif
