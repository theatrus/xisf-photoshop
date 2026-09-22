#pragma once
#include "options.h"
#include "preferences.h"
#include <cstring>
#include <string>

#ifdef _WIN32
struct SeizaDialogData { const wchar_t* title; const wchar_t* message; uint32_t depth; bool* remember; uint32_t* debayer; };
inline INT_PTR CALLBACK seizaOptionsProc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
    if (message == WM_INITDIALOG) {
        const auto& data = *reinterpret_cast<SeizaDialogData*>(lparam);
        SetWindowLongPtrW(window, DWLP_USER, lparam);
        SetWindowTextW(window, data.title);
        SetDlgItemTextW(window, SEIZA_OPTIONS_TEXT, data.message);
        CheckRadioButton(window, SEIZA_FLOAT_CHOICE, SEIZA_INTEGER_CHOICE,
            data.depth == 16 ? SEIZA_INTEGER_CHOICE : SEIZA_FLOAT_CHOICE);
        ShowWindow(GetDlgItem(window, SEIZA_REMEMBER_CHOICE), data.remember ? SW_SHOW : SW_HIDE);
        CheckDlgButton(window, SEIZA_REMEMBER_CHOICE, BST_UNCHECKED);
        for (int control : {SEIZA_DEBAYER_LABEL, SEIZA_DEBAYER_CHOICE})
            ShowWindow(GetDlgItem(window, control), data.debayer ? SW_SHOW : SW_HIDE);
        if (data.debayer) {
            for (const auto* label : {L"Keep raw grayscale", L"Debayer to RGB - Auto (metadata)",
                L"Debayer to RGB - RGGB", L"Debayer to RGB - BGGR", L"Debayer to RGB - GRBG", L"Debayer to RGB - GBRG"})
                SendDlgItemMessageW(window, SEIZA_DEBAYER_CHOICE, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(label));
            SendDlgItemMessageW(window, SEIZA_DEBAYER_CHOICE, CB_SETCURSEL, *data.debayer, 0);
        }
        return TRUE;
    }
    if (message == WM_COMMAND) {
        if (LOWORD(wparam) == IDOK) {
            auto* data = reinterpret_cast<SeizaDialogData*>(GetWindowLongPtrW(window, DWLP_USER));
            if (data->remember) *data->remember = IsDlgButtonChecked(window, SEIZA_REMEMBER_CHOICE) == BST_CHECKED;
            if (data->debayer) *data->debayer = static_cast<uint32_t>(SendDlgItemMessageW(window, SEIZA_DEBAYER_CHOICE, CB_GETCURSEL, 0, 0));
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
            if (control == SEIZA_EXPORT_DEFAULT)
                SendDlgItemMessageW(window, control, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L"Match document depth"));
            SendDlgItemMessageW(window, control, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L"32-bit floating point"));
            SendDlgItemMessageW(window, control, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L"16-bit integer"));
        }
        SendDlgItemMessageW(window, SEIZA_IMPORT_DEFAULT, CB_SETCURSEL, data->readDepth == 16 ? 1 : 0, 0);
        SendDlgItemMessageW(window, SEIZA_EXPORT_DEFAULT, CB_SETCURSEL,
            data->writeDepth == 16 ? 2 : data->writeDepth == 32 ? 1 : 0, 0);
        CheckDlgButton(window, SEIZA_ASK_OPEN, data->askOnOpen ? BST_CHECKED : BST_UNCHECKED);
        CheckDlgButton(window, SEIZA_ASK_SAVE, data->askOnSave ? BST_CHECKED : BST_UNCHECKED);
        CheckDlgButton(window, SEIZA_DEBAYER_DEFAULT, data->debayer ? BST_CHECKED : BST_UNCHECKED);
        return TRUE;
    }
    if (message == WM_COMMAND) {
        if (LOWORD(wparam) == IDOK) {
            auto* data = reinterpret_cast<SeizaDefaults*>(GetWindowLongPtrW(window, DWLP_USER));
            data->readDepth = SendDlgItemMessageW(window, SEIZA_IMPORT_DEFAULT, CB_GETCURSEL, 0, 0) == 1 ? 16 : 32;
            const auto savedType = SendDlgItemMessageW(window, SEIZA_EXPORT_DEFAULT, CB_GETCURSEL, 0, 0);
            data->writeDepth = savedType == 2 ? 16 : savedType == 1 ? 32 : 0;
            data->askOnOpen = IsDlgButtonChecked(window, SEIZA_ASK_OPEN) == BST_CHECKED;
            data->askOnSave = IsDlgButtonChecked(window, SEIZA_ASK_SAVE) == BST_CHECKED;
            data->debayer = IsDlgButtonChecked(window, SEIZA_DEBAYER_DEFAULT) == BST_CHECKED ? 1 : 0;
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
uint32_t chooseDepthMac(const char* title, const std::string& message, uint32_t initial, bool* remember, uint32_t* debayer);
#endif

// Zero means Cancel. Call only from interactive read/options selectors.
inline uint32_t chooseDepth(const char* title, const std::string& message, uint32_t initial, bool* remember = nullptr, uint32_t* debayer = nullptr) {
    if (remember) *remember = false;
#ifdef _WIN32
    HMODULE module = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        reinterpret_cast<LPCWSTR>(&seizaOptionsProc), &module))
        throw std::runtime_error("Cannot locate the plugin options dialog");
    const std::wstring wideTitle(title, title + std::strlen(title));
    const std::wstring wideMessage(message.begin(), message.end());
    SeizaDialogData data{wideTitle.c_str(), wideMessage.c_str(), initial, remember, debayer};
    const auto result = DialogBoxParamW(module, MAKEINTRESOURCEW(SEIZA_OPTIONS_DIALOG), GetActiveWindow(),
        seizaOptionsProc, reinterpret_cast<LPARAM>(&data));
    if (result == -1) throw std::runtime_error("Cannot display the plugin options dialog");
    return static_cast<uint32_t>(result);
#else
    return chooseDepthMac(title, message, initial, remember, debayer);
#endif
}
