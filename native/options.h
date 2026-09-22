#pragma once
#include <cstdint>

// Versioned document options stored in a Photoshop-owned revertInfo handle.
struct SeizaOptions {
    uint32_t magic = 0x535A4F50; // SZOP
    uint32_t version = 1;
    uint32_t readDepth = 32;
    uint32_t writeDepth = 32;
};

#define SEIZA_OPTIONS_DIALOG 17000
#define SEIZA_OPTIONS_TEXT 17001
#define SEIZA_FLOAT_CHOICE 17002
#define SEIZA_INTEGER_CHOICE 17003
