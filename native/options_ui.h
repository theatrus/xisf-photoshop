#pragma once
#include "options.h"
#include "preferences.h"
#include <cstring>
#include <string>

#ifdef _WIN32
struct SeizaDialogData { const wchar_t* title; const wchar_t* message; uint32_t depth; };
inline INT_PTR CALLBACK seizaOptionsProc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
    if (message == WM_INITDIALOG) {
        const auto& data = *reinterpret_cast<SeizaDialogData*>(lparam);
        SetWindowTextW(window, data.title);
        SetDlgItemTextW(window, SEIZA_OPTIONS_TEXT, data.message);
        CheckRadioButton(window, SEIZA_FLOAT_CHOICE, SEIZA_INTEGER_CHOICE,
            data.depth == 16 ? SEIZA_INTEGER_CHOICE : SEIZA_FLOAT_CHOICE);
        return TRUE;
    }
    if (message == WM_COMMAND) {
        if (LOWORD(wparam) == IDOK) {
            EndDialog(window, IsDlgButtonChecked(window, SEIZA_INTEGER_CHOICE) == BST_CHECKED ? 16 : 32);
            return TRUE;
        }
        if (LOWORD(wparam) == IDCANCEL) { EndDialog(window, 0); return TRUE; }
    }
    if (message == WM_CLOSE) { EndDialog(window, 0); return TRUE; }
    return FALSE;
}

inline INT_PTR CALLBACK seizaSettingsProc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
    if (message == WM_INITDIALOG) {
        auto* data = reinterpret_cast<SeizaDefaults*>(lparam);
        SetWindowLongPtrW(window, DWLP_USER, lparam);
        for (int control : {SEIZA_IMPORT_DEFAULT, SEIZA_EXPORT_DEFAULT}) {
            SendDlgItemMessageW(window, control, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L"32-bit floating point"));
            SendDlgItemMessageW(window, control, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L"16-bit integer"));
        }
        SendDlgItemMessageW(window, SEIZA_IMPORT_DEFAULT, CB_SETCURSEL, data->readDepth == 16 ? 1 : 0, 0);
        SendDlgItemMessageW(window, SEIZA_EXPORT_DEFAULT, CB_SETCURSEL, data->writeDepth == 16 ? 1 : 0, 0);
        CheckDlgButton(window, SEIZA_ASK_OPEN, data->askOnOpen ? BST_CHECKED : BST_UNCHECKED);
        CheckDlgButton(window, SEIZA_ASK_SAVE, data->askOnSave ? BST_CHECKED : BST_UNCHECKED);
        return TRUE;
    }
    if (message == WM_COMMAND) {
        if (LOWORD(wparam) == IDOK) {
            auto* data = reinterpret_cast<SeizaDefaults*>(GetWindowLongPtrW(window, DWLP_USER));
            data->readDepth = SendDlgItemMessageW(window, SEIZA_IMPORT_DEFAULT, CB_GETCURSEL, 0, 0) == 1 ? 16 : 32;
            data->writeDepth = SendDlgItemMessageW(window, SEIZA_EXPORT_DEFAULT, CB_GETCURSEL, 0, 0) == 1 ? 16 : 32;
            data->askOnOpen = IsDlgButtonChecked(window, SEIZA_ASK_OPEN) == BST_CHECKED;
            data->askOnSave = IsDlgButtonChecked(window, SEIZA_ASK_SAVE) == BST_CHECKED;
            EndDialog(window, IDOK); return TRUE;
        }
        if (LOWORD(wparam) == IDCANCEL) { EndDialog(window, IDCANCEL); return TRUE; }
    }
    if (message == WM_CLOSE) { EndDialog(window, IDCANCEL); return TRUE; }
    return FALSE;
}

inline bool editDefaults(SeizaDefaults& defaults) {
    HMODULE module = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        reinterpret_cast<LPCWSTR>(&seizaSettingsProc), &module))
        throw std::runtime_error("Cannot locate the plugin settings dialog");
    const auto result = DialogBoxParamW(module, MAKEINTRESOURCEW(SEIZA_SETTINGS_DIALOG), GetActiveWindow(),
        seizaSettingsProc, reinterpret_cast<LPARAM>(&defaults));
    if (result == -1) throw std::runtime_error("Cannot display the plugin settings dialog");
    return result == IDOK;
}
#else
bool editDefaults(SeizaDefaults& defaults);
#endif

// Zero means Cancel. Call only from interactive read/options selectors.
inline uint32_t chooseDepth(const char* title, const std::string& message, uint32_t initial) {
#ifdef _WIN32
    HMODULE module = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        reinterpret_cast<LPCWSTR>(&seizaOptionsProc), &module))
        throw std::runtime_error("Cannot locate the plugin options dialog");
    const std::wstring wideTitle(title, title + std::strlen(title));
    const std::wstring wideMessage(message.begin(), message.end());
    SeizaDialogData data{wideTitle.c_str(), wideMessage.c_str(), initial};
    const auto result = DialogBoxParamW(module, MAKEINTRESOURCEW(SEIZA_OPTIONS_DIALOG), GetActiveWindow(),
        seizaOptionsProc, reinterpret_cast<LPARAM>(&data));
    if (result == -1) throw std::runtime_error("Cannot display the plugin options dialog");
    return static_cast<uint32_t>(result);
#else
    CFStringRef heading = CFStringCreateWithCString(nullptr, title, kCFStringEncodingUTF8);
    CFStringRef text = CFStringCreateWithCString(nullptr, message.c_str(), kCFStringEncodingUTF8);
    if (!heading || !text) {
        if (heading) CFRelease(heading);
        if (text) CFRelease(text);
        throw std::bad_alloc();
    }
    CFOptionFlags response = kCFUserNotificationCancelResponse;
    const auto status = CFUserNotificationDisplayAlert(0, kCFUserNotificationCautionAlertLevel,
        nullptr, nullptr, nullptr, heading, text,
        initial == 16 ? CFSTR("16-bit integer") : CFSTR("32-bit float"),
        initial == 16 ? CFSTR("32-bit float") : CFSTR("16-bit integer"), CFSTR("Cancel"), &response);
    CFRelease(heading); CFRelease(text);
    if (status) throw std::runtime_error("Cannot display the plugin options dialog");
    if (response == kCFUserNotificationDefaultResponse) return initial;
    if (response == kCFUserNotificationAlternateResponse) return initial == 16 ? 32 : 16;
    return 0;
#endif
}
