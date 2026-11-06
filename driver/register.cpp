#include <windows.h>
#include <cfg.h>
#include <cfgmgr32.h>
#include <initguid.h>
#include <devguid.h>
#include <devpkey.h>
#include <newdev.h>
#include <setupapi.h>
#include <cwchar>
#include <filesystem>
#include <string_view>

namespace
{
constexpr wchar_t HardwareId[] = L"ROOT\\MPKDeckVoice";
constexpr wchar_t HardwareIds[] = L"ROOT\\MPKDeckVoice\0"; // REG_MULTI_SZ
constexpr int RestartRequired = 3010;

enum class Find { missing, present, error };

Find FindDevice(HDEVINFO devices, SP_DEVINFO_DATA& match)
{
    for (DWORD index = 0; ; ++index)
    {
        SP_DEVINFO_DATA info{};
        info.cbSize = sizeof(info);
        if (!SetupDiEnumDeviceInfo(devices, index, &info))
        {
            if (GetLastError() != ERROR_NO_MORE_ITEMS) return Find::error;
            return match.cbSize ? Find::present : Find::missing;
        }
        wchar_t ids[512]{};
        DWORD type = 0;
        DWORD bytes = 0;
        if (!SetupDiGetDeviceRegistryPropertyW(devices, &info, SPDRP_HARDWAREID, &type,
                reinterpret_cast<BYTE*>(ids), sizeof(ids) - sizeof(wchar_t), &bytes))
        {
            const DWORD error = GetLastError();
            if (error == ERROR_INVALID_DATA || error == ERROR_INSUFFICIENT_BUFFER) continue;
            return Find::error;
        }
        if (type != REG_MULTI_SZ || bytes < sizeof(wchar_t) * 2) continue;
        for (const wchar_t* id = ids; *id; id += std::wcslen(id) + 1)
        {
            if (_wcsicmp(id, HardwareId) != 0) continue;
            if (match.cbSize) return Find::error;
            match = info;
            break;
        }
    }
}

bool RemoveNode(HDEVINFO devices, SP_DEVINFO_DATA& info, bool& restart)
{
    SP_REMOVEDEVICE_PARAMS parameters{};
    parameters.ClassInstallHeader.cbSize = sizeof(parameters.ClassInstallHeader);
    parameters.ClassInstallHeader.InstallFunction = DIF_REMOVE;
    parameters.Scope = DI_REMOVEDEVICE_GLOBAL;
    if (!SetupDiSetClassInstallParamsW(devices, &info, &parameters.ClassInstallHeader, sizeof(parameters))
        || !SetupDiCallClassInstaller(DIF_REMOVE, devices, &info))
        return false;
    SP_DEVINSTALL_PARAMS_W result{};
    result.cbSize = sizeof(result);
    restart = SetupDiGetDeviceInstallParamsW(devices, &info, &result)
        && (result.Flags & (DI_NEEDRESTART | DI_NEEDREBOOT));
    return true;
}

bool CreateNode(HDEVINFO devices, SP_DEVINFO_DATA& info)
{
    info.cbSize = sizeof(info);
    return SetupDiCreateDeviceInfoW(devices, L"MEDIA", &GUID_DEVCLASS_MEDIA, nullptr, nullptr, DICD_GENERATE_ID, &info)
        && SetupDiSetDeviceRegistryPropertyW(devices, &info, SPDRP_HARDWAREID,
               reinterpret_cast<const BYTE*>(HardwareIds), sizeof(HardwareIds))
        && SetupDiCallClassInstaller(DIF_REGISTERDEVICE, devices, &info);
}

bool IsStarted(const SP_DEVINFO_DATA& info)
{
    ULONG status = 0, problem = 0;
    return CM_Get_DevNode_Status(&status, &problem, info.DevInst, 0) == CR_SUCCESS && (status & DN_STARTED);
}

int Install(HDEVINFO devices, SP_DEVINFO_DATA& info, Find found, const std::filesystem::path& inf)
{
    const bool create = found == Find::missing;
    if (create && !CreateNode(devices, info)) return 11;
    BOOL restart = FALSE;
    if (!UpdateDriverForPlugAndPlayDevicesW(nullptr, HardwareId, inf.c_str(), INSTALLFLAG_NONINTERACTIVE, &restart))
    {
        if (!create && GetLastError() == ERROR_NO_MORE_ITEMS && IsStarted(info)) return 0;
        if (create)
        {
            bool rollbackRestart = false;
            if (!RemoveNode(devices, info, rollbackRestart)) return 13;
        }
        return 12;
    }
    return restart ? RestartRequired : 0;
}

int Uninstall(HDEVINFO devices, SP_DEVINFO_DATA& info, Find found)
{
    if (found == Find::missing) return 0;
    wchar_t published[MAX_PATH]{};
    DEVPROPTYPE type = 0;
    const bool driverKnown = SetupDiGetDevicePropertyW(devices, &info, &DEVPKEY_Device_DriverInfPath, &type,
        reinterpret_cast<BYTE*>(published), sizeof(published) - sizeof(wchar_t), nullptr, 0)
        && type == DEVPROP_TYPE_STRING;
    bool restart = false;
    if (!RemoveNode(devices, info, restart)) return 21;
    // Limit deletion to the published INF associated with this exact device.
    std::wstring_view name(published);
    const bool oemInf = name.starts_with(L"oem") && name.ends_with(L".inf") && name.size() > 7
        && name.substr(3, name.size() - 7).find_first_not_of(L"0123456789") == name.npos;
    if (driverKnown && oemInf && !SetupUninstallOEMInfW(published, 0, nullptr)
        && GetLastError() != ERROR_FILE_NOT_FOUND)
        return restart ? RestartRequired : 22;
    return restart ? RestartRequired : 0;
}
}

int wmain(int argc, wchar_t** argv)
{
    if (argc != 2) return 2;
    const std::wstring_view action(argv[1]);
    if (action != L"install" && action != L"remove") return 2;
    wchar_t path[32768]{};
    const DWORD length = GetModuleFileNameW(nullptr, path, ARRAYSIZE(path));
    if (!length || length == ARRAYSIZE(path)) return 3;
    const auto inf = std::filesystem::path(path).parent_path() / L"MPKDeckVoice.inf";
    std::error_code error;
    if (action == L"install" && !std::filesystem::is_regular_file(inf, error)) return 3;

    HDEVINFO devices = SetupDiGetClassDevsW(&GUID_DEVCLASS_MEDIA, nullptr, nullptr, DIGCF_PRESENT);
    if (devices == INVALID_HANDLE_VALUE) return 4;
    SP_DEVINFO_DATA info{};
    const auto found = FindDevice(devices, info);
    const auto result = found == Find::error ? 5 :
        action == L"install" ? Install(devices, info, found, inf) : Uninstall(devices, info, found);
    SetupDiDestroyDeviceInfoList(devices);
    return result;
}
