#pragma once
#include <cstdint>

// Versioned document options stored in a Photoshop-owned revertInfo handle.
struct SeizaOptions {
    uint32_t magic = 0x535A4F50; // SZOP
    uint32_t version = 4;
    uint32_t readDepth = 32;
    uint32_t writeDepth = 0; // Zero follows the current Photoshop document depth.
    uint32_t debayer = 1; // 0=raw, 1=metadata, 2..5=manual RGGB/BGGR/GRBG/GBRG.
    uint32_t removeAstrometry = 2; // 0/1 are document choices; 2 follows current defaults.
};

#define SEIZA_OPTIONS_DIALOG 17000
#define SEIZA_OPTIONS_TEXT 17001
#define SEIZA_FLOAT_CHOICE 17002
#define SEIZA_INTEGER_CHOICE 17003
#define SEIZA_REMEMBER_CHOICE 17004
#define SEIZA_DEBAYER_LABEL 17005
#define SEIZA_DEBAYER_CHOICE 17006
#define SEIZA_REMOVE_ASTROMETRY 17007
#define SEIZA_ASTROMETRY_HINT 17008
#define SEIZA_SETTINGS_DIALOG 17100
#define SEIZA_IMPORT_DEFAULT 17101
#define SEIZA_EXPORT_DEFAULT 17102
#define SEIZA_ASK_OPEN 17103
#define SEIZA_ASK_SAVE 17104
#define SEIZA_DEBAYER_DEFAULT 17105
#define SEIZA_ASTROMETRY_DEFAULT 17106
