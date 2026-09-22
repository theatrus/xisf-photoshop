#pragma once
#include <cstdint>

// Versioned document options stored in a Photoshop-owned revertInfo handle.
struct SeizaOptions {
    uint32_t magic = 0x535A4F50; // SZOP
    uint32_t version = 2;
    uint32_t readDepth = 32;
    uint32_t writeDepth = 0; // Zero follows the current Photoshop document depth.
};

#define SEIZA_OPTIONS_DIALOG 17000
#define SEIZA_OPTIONS_TEXT 17001
#define SEIZA_FLOAT_CHOICE 17002
#define SEIZA_INTEGER_CHOICE 17003
#define SEIZA_SETTINGS_DIALOG 17100
#define SEIZA_IMPORT_DEFAULT 17101
#define SEIZA_EXPORT_DEFAULT 17102
#define SEIZA_ASK_OPEN 17103
#define SEIZA_ASK_SAVE 17104
