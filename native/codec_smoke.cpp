// Uses the actual C ABI and release Rust library, without a Photoshop SDK.
#include "seiza_codec.h"
#include <cstdio>
#include <cstring>
#include <vector>

static int32_t append(void* context, const uint8_t* bytes, size_t count) {
    auto& output = *static_cast<std::vector<uint8_t>*>(context);
    output.insert(output.end(), bytes, bytes + count);
    return 0;
}
int main() {
    const float pixels[] = {-0.25f, 0.0f, 0.5f, 1.0f, 2.0f, 100.0f};
    for (uint32_t format : {1u, 2u}) {
        char error[1024]{};
        std::vector<uint8_t> encoded;
        if (seiza_encode(format, 2, 1, 3, pixels, 6, append, &encoded, error, sizeof(error))) {
            std::fprintf(stderr, "%s\n", error); return 1;
        }
        SeizaImage* image = nullptr;
        if (seiza_decode(format, encoded.data(), encoded.size(), &image, error, sizeof(error))) {
            std::fprintf(stderr, "%s\n", error); return 2;
        }
        SeizaImageView view{};
        const bool valid = seiza_image_view(image, &view) == 0 && view.width == 2 &&
            view.height == 1 && view.planes == 3 && view.samples == 6 &&
            std::memcmp(view.pixels, pixels, sizeof(pixels)) == 0;
        seiza_image_free(image);
        if (!valid) return 3;
    }
    std::puts("C++/Rust ABI: FITS and XISF float RGB round trips passed.");
}
