#pragma once
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <string>
#ifdef _WIN32
#include <windows.h>
#else
#include <unistd.h>
#endif

struct SeizaDefaults {
    uint32_t readDepth = 32;
    uint32_t writeDepth = 0; // Match the document; 16/32 are explicit overrides.
    bool askOnOpen = true;
    bool askOnSave = false;
    uint32_t debayer = 1; // Auto for recognized metadata. Never remember a forced pattern globally.
};

inline std::filesystem::path preferencesPath() {
#ifdef _WIN32
    auto variable = [](const wchar_t* name) {
        const DWORD length = GetEnvironmentVariableW(name, nullptr, 0);
        if (!length) return std::wstring{};
        std::wstring value(length, L'\0');
        const DWORD used = GetEnvironmentVariableW(name, value.data(), length);
        if (!used || used >= length) throw std::runtime_error("Cannot locate Seiza preferences");
        value.resize(used);
        return value;
    };
    const auto overridePath = variable(L"SEIZA_PHOTOSHOP_PREFERENCES");
    if (!overridePath.empty()) return overridePath;
    const auto home = variable(L"APPDATA");
    if (home.empty()) throw std::runtime_error("Cannot locate your application preferences folder");
    return std::filesystem::path(home) / L"Seiza" / L"Photoshop" / L"defaults-v1.txt";
#else
    const char* overridePath = std::getenv("SEIZA_PHOTOSHOP_PREFERENCES");
    if (overridePath && *overridePath) return overridePath;
    const char* home = std::getenv("HOME");
    if (!home || !*home) throw std::runtime_error("Cannot locate your application preferences folder");
    return std::filesystem::path(home) / "Library/Application Support/Seiza/Photoshop/defaults-v1.txt";
#endif
}

inline SeizaDefaults readDefaults() noexcept {
    try {
        std::ifstream input(preferencesPath());
        std::string signature, trailing;
        unsigned read = 0, write = 0, askOpen = 0, askSave = 0;
        unsigned debayer = 0; // Preserve raw imports when migrating older preferences.
        if (!(input >> signature >> read >> write >> askOpen >> askSave)) return {};
        if (signature == "SEIZA_DEFAULTS_V3" && !(input >> debayer)) return {};
        if ((signature == "SEIZA_DEFAULTS_V1" || signature == "SEIZA_DEFAULTS_V2" || signature == "SEIZA_DEFAULTS_V3") &&
            (read == 16 || read == 32) && (write == 16 || write == 32 ||
            (write == 0 && signature != "SEIZA_DEFAULTS_V1")) &&
            askOpen <= 1 && askSave <= 1 && debayer <= 1 && !(input >> trailing))
            // Old settings seeded a fixed save depth even without an explicit choice.
            return {read, signature == "SEIZA_DEFAULTS_V1" ? 0u : write, askOpen != 0, askSave != 0, debayer};
    } catch (...) { /* Missing/unreadable/corrupt preferences use factory defaults. */ }
    return {};
}

inline void saveDefaults(const SeizaDefaults& defaults) {
    if ((defaults.readDepth != 16 && defaults.readDepth != 32) ||
        (defaults.writeDepth != 0 && defaults.writeDepth != 16 && defaults.writeDepth != 32) || defaults.debayer > 1)
        throw std::runtime_error("Invalid Seiza default sample type");
    const auto path = preferencesPath();
    if (path.has_parent_path()) std::filesystem::create_directories(path.parent_path());
    // One modal settings dialog per Photoshop process. Separate processes get
    // separate staging files; an atomic replacement publishes one whole record.
#ifdef _WIN32
    const auto pid = GetCurrentProcessId();
#else
    const auto pid = getpid();
#endif
    auto temporary = path;
    temporary += "." + std::to_string(pid) + ".tmp";
    try {
        {
            std::ofstream output(temporary, std::ios::trunc);
            output << "SEIZA_DEFAULTS_V3\n" << defaults.readDepth << ' ' << defaults.writeDepth
                   << ' ' << defaults.askOnOpen << ' ' << defaults.askOnSave << ' ' << defaults.debayer << '\n';
            output.flush();
            if (!output) throw std::runtime_error("Cannot write Seiza preferences");
            output.close();
            if (!output) throw std::runtime_error("Cannot finish writing Seiza preferences");
        }
#ifdef _WIN32
        if (!MoveFileExW(temporary.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH))
            throw std::runtime_error("Cannot replace Seiza preferences");
#else
        std::filesystem::rename(temporary, path);
#endif
    } catch (...) {
        std::error_code ignored;
        std::filesystem::remove(temporary, ignored);
        throw;
    }
}

inline void rememberImportChoice(uint32_t depth, uint32_t debayer = UINT32_MAX) {
    auto defaults = readDefaults();
    defaults.readDepth = depth;
    defaults.askOnOpen = false;
    if (debayer != UINT32_MAX) defaults.debayer = debayer ? 1 : 0;
    saveDefaults(defaults);
}
