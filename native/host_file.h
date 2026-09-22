#pragma once
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <stdexcept>

#ifdef _WIN32
#include <windows.h>
#else
#include <cerrno>
#include <sys/stat.h>
#include <unistd.h>
#endif

// Borrow Photoshop's open file. Never close it or reopen by pathname: the host
// owns its lifecycle, including temporary files used by Save As.
class HostFile {
public:
    explicit HostFile(FormatRecord& record) {
#ifdef _WIN32
        handle_ = reinterpret_cast<HANDLE>(record.dataFork);
        if (!handle_ || handle_ == INVALID_HANDLE_VALUE) fail("Photoshop did not supply a file handle");
#else
        if (!record.hostSupportsPOSIXIO || !record.pluginUsingPOSIXIO || record.posixFileDescriptor < 0)
            fail("Photoshop did not supply a POSIX file descriptor");
        descriptor_ = record.posixFileDescriptor;
#endif
    }
    int64_t size() const {
#ifdef _WIN32
        LARGE_INTEGER result{};
        if (!GetFileSizeEx(handle_, &result)) fail("Cannot determine file size");
        return result.QuadPart;
#else
        struct stat result{};
        if (fstat(descriptor_, &result)) fail("Cannot determine file size");
        return result.st_size;
#endif
    }
    int64_t position() const {
#ifdef _WIN32
        LARGE_INTEGER zero{}, result{};
        if (!SetFilePointerEx(handle_, zero, &result, FILE_CURRENT)) fail("Cannot get file position");
        return result.QuadPart;
#else
        const auto result = lseek(descriptor_, 0, SEEK_CUR);
        if (result < 0) fail("Cannot get file position");
        return result;
#endif
    }
    void seek(int64_t position) const {
#ifdef _WIN32
        LARGE_INTEGER target{}; target.QuadPart = position;
        if (!SetFilePointerEx(handle_, target, nullptr, FILE_BEGIN)) fail("Cannot seek astronomy file");
#else
        if (lseek(descriptor_, position, SEEK_SET) < 0) fail("Cannot seek astronomy file");
#endif
    }
    size_t read(void* bytes, size_t count) const {
#ifdef _WIN32
        DWORD received = 0;
        if (!ReadFile(handle_, bytes, static_cast<DWORD>(count), &received, nullptr)) fail("Cannot read astronomy file");
        return received;
#else
        ssize_t received;
        do { received = ::read(descriptor_, bytes, count); } while (received < 0 && errno == EINTR);
        if (received < 0) fail("Cannot read astronomy file");
        return static_cast<size_t>(received);
#endif
    }
    size_t write(const void* bytes, size_t count) const {
#ifdef _WIN32
        DWORD written = 0;
        if (!WriteFile(handle_, bytes, static_cast<DWORD>(count), &written, nullptr)) fail("Cannot write astronomy file");
        return written;
#else
        ssize_t written;
        do { written = ::write(descriptor_, bytes, count); } while (written < 0 && errno == EINTR);
        if (written < 0) fail("Cannot write astronomy file");
        return static_cast<size_t>(written);
#endif
    }
    void finish() const {
#ifdef _WIN32
        if (!SetEndOfFile(handle_)) fail("Cannot finish astronomy file");
#else
        if (ftruncate(descriptor_, position())) fail("Cannot finish astronomy file");
#endif
    }
private:
    [[noreturn]] static void fail(const char* message) { throw std::runtime_error(message); }
#ifdef _WIN32
    HANDLE handle_{};
#else
    int descriptor_ = -1;
#endif
};
