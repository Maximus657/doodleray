!include LogicLib.nsh

!macro DoodleRayExecBestEffort COMMAND LABEL TIMEOUT
  DetailPrint "${LABEL}"
  nsExec::ExecToStack /TIMEOUT=${TIMEOUT} '${COMMAND}'
  Pop $0
  Pop $1
  DetailPrint "$1"
  ${If} $0 != 0
    DetailPrint "${LABEL} failed or timed out (exit=$0). Continuing so DoodleRay update can finish."
  ${EndIf}
!macroend

!macro DoodleRayExecRequired COMMAND LABEL
  DetailPrint "${LABEL}"
  ClearErrors
  ExecWait '${COMMAND}' $0
  ${If} ${Errors}
    Abort "${LABEL} could not be started. Please reinstall DoodleRay from the official installer."
  ${ElseIf} $0 != 0
    Abort "${LABEL} failed (exit=$0). Please reinstall DoodleRay from the official installer."
  ${EndIf}
!macroend

!macro DoodleRayRequireFile RELATIVE_PATH LABEL
  ${If} ${FileExists} "$INSTDIR\${RELATIVE_PATH}"
    DetailPrint "${LABEL}: found"
  ${Else}
    Abort "${LABEL} is missing. Please reinstall DoodleRay from the official installer."
  ${EndIf}
!macroend

!macro DoodleRayKillOwnedProcesses
  ; xray.exe/sing-box.exe run under DoodleRayTunnelService in Protected/TUN
  ; mode, but under the main DoodleRay.exe app directly in Browsers/Manual
  ; (system-proxy) mode. Stopping the service alone leaves that second case's
  ; engine process (and the app itself, if still open) holding a file lock,
  ; which fails the installer with "Error opening file for writing" on
  ; xray.exe/sing-box.exe/DoodleRay.exe. Force-kill all of them by name as a
  ; safety net regardless of which mode spawned them; a process that isn't
  ; running just makes taskkill fail harmlessly.
  nsExec::ExecToLog /TIMEOUT=5000 'taskkill /F /IM DoodleRay.exe /T'
  nsExec::ExecToLog /TIMEOUT=5000 'taskkill /F /IM xray.exe /T'
  nsExec::ExecToLog /TIMEOUT=5000 'taskkill /F /IM sing-box.exe /T'
!macroend

!macro DoodleRayRemoveLegacyUserInstall
  ; v5 could leave a separate current-user installation behind when the v6
  ; installer moved to Program Files. Tauri's per-machine template only sees
  ; HKLM, so retire the old HKCU entry explicitly.
  ;
  ; Do not ExecWait or recursively delete this location: it is user-controlled
  ; data while this hook is elevated. Run the fixed legacy uninstaller as the
  ; interactive user instead. Silent NSIS uninstall keeps app data unless the
  ; user explicitly selected its delete-data checkbox.
  ReadRegStr $0 HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\DoodleRay" "InstallLocation"
  ${If} $0 != ""
    StrCpy $1 $0 1
    ${If} $1 == "$\""
      StrCpy $1 $0 -1
      ${If} $1 == "$\""
        StrCpy $0 $0 -1 1
      ${Else}
        StrCpy $0 ""
      ${EndIf}
    ${EndIf}
    ${If} $0 != ""
    ${AndIf} ${FileExists} "$0\\DoodleRay.exe"
    ${AndIf} ${FileExists} "$0\\uninstall.exe"
      DetailPrint "Removing legacy per-user DoodleRay installation..."
      nsis_tauri_utils::RunAsUser "$0\\uninstall.exe" "/S"
      ; RunAsUser is intentionally asynchronous. The legacy uninstaller also
      ; owns DoodleRayTunnelService, so do not install the new service until
      ; its uninstaller has removed itself.
      StrCpy $2 0
      doodleray_legacy_uninstall_wait:
        Sleep 500
        ${IfNot} ${FileExists} "$0\\uninstall.exe"
          Goto doodleray_legacy_uninstall_done
        ${EndIf}
        IntOp $2 $2 + 1
        ${If} $2 < 120
          Goto doodleray_legacy_uninstall_wait
        ${EndIf}
        Abort "Legacy per-user DoodleRay cleanup timed out. Close the old copy and retry the official installer."
      doodleray_legacy_uninstall_done:
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Preparing DoodleRay Tunnel Service for update..."
  ; Do not call the previously installed DoodleRayService.exe here: older
  ; service builds used a different pipe/protocol and can block the updater.
  ; The app calls PrepareForUpdate before launching the updater; this hook is a
  ; last-resort SCM cleanup so installer replacement never depends on old code.
  !insertmacro DoodleRayKillOwnedProcesses
  nsExec::ExecToLog /TIMEOUT=10000 'sc stop DoodleRayTunnelService'
  Sleep 1000
  nsExec::ExecToLog /TIMEOUT=10000 'sc stop DoodleRayTunnelService'
  nsExec::ExecToLog /TIMEOUT=10000 'sc delete DoodleRayTunnelService'
  Sleep 2000
  !insertmacro DoodleRayRemoveLegacyUserInstall
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro DoodleRayRequireFile "DoodleRayService.exe" "DoodleRay Tunnel Service"
  !insertmacro DoodleRayRequireFile "sing-box.exe" "sing-box runtime"
  !insertmacro DoodleRayRequireFile "wintun.dll" "Wintun driver runtime"
  !insertmacro DoodleRayRequireFile "xray-core\xray.exe" "xray-core runtime"
  !insertmacro DoodleRayExecRequired '"$INSTDIR\DoodleRayService.exe" install' "Installing DoodleRay Tunnel Service"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Removing DoodleRay Tunnel Service..."
  !insertmacro DoodleRayKillOwnedProcesses
  nsExec::ExecToLog /TIMEOUT=10000 'sc stop DoodleRayTunnelService'
  Sleep 1000
  nsExec::ExecToLog /TIMEOUT=10000 'sc stop DoodleRayTunnelService'
  nsExec::ExecToLog /TIMEOUT=10000 'sc delete DoodleRayTunnelService'
!macroend
