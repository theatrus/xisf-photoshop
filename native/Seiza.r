#include "PIDefines.h"
#include "PIGeneral.h"

#if SEIZA_FORMAT == 1
#define FORMAT_NAME "FITS"
#define FILE_TYPE 'FITS'
#else
#define FORMAT_NAME "XISF"
#define FILE_TYPE 'XISF'
#endif

resource 'PiPL' (16000, FORMAT_NAME, purgeable) {
    {
        Kind { ImageFormat },
        Name { FORMAT_NAME },
        Category { "Seiza Astronomy" },
        Version { (latestFormatVersion << 16) | latestFormatSubVersion },
        CodeWin64X86 { "PluginMain" },
        SupportedModes {
            noBitmap, doesSupportGrayScale, noIndexedColor, doesSupportRGBColor,
            noCMYKColor, noHSLColor, noHSBColor, noMultichannel, noDuotone, noLABColor
        },
        EnableInfo { "in (PSHOP_ImageMode, Gray16Mode, RGB48Mode, Gray32Mode, RGB96Mode)" },
        FmtFileType { FILE_TYPE, '8BIM' },
#if SEIZA_FORMAT == 1
        ReadExtensions { { 'fits', 'fit ', 'fts ' } },
        WriteExtensions { { 'fits', 'fit ', 'fts ' } },
        FilteredExtensions { { 'fits', 'fit ', 'fts ' } },
#else
        ReadExtensions { { 'xisf' } },
        WriteExtensions { { 'xisf' } },
        FilteredExtensions { { 'xisf' } },
#endif
        FormatFlags { fmtDoesNotSaveImageResources, fmtCanRead, fmtCanWrite,
            fmtCanWriteIfRead, fmtCannotWriteTransparency, fmtCannotCreateThumbnail },
        PlugInMaxSize { 300000, 300000 },
        FormatMaxSize { { 32767, 32767 } },
        FormatMaxChannels { { 0, 1, 0, 3, 0, 0, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 3, 1 } },
#if SEIZA_FORMAT == 2
        FormatICCFlags { iccCanEmbedGray, iccCannotEmbedIndexed,
            iccCanEmbedRGB, iccCannotEmbedCMYK }
#else
        FormatICCFlags { iccCannotEmbedGray, iccCannotEmbedIndexed,
            iccCannotEmbedRGB, iccCannotEmbedCMYK }
#endif
    }
};
