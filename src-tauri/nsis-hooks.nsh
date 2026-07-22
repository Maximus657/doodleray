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

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Preparing DoodleRay Tunnel Service for update..."
  ; Do not call the previously installed DoodleRayService.exe here: older
  ; service builds used a different pipe/protocol and can block the updater.
  ; The app calls PrepareForUpdate before launching the updater; this hook is a
  ; last-resort SCM cleanup so installer replacement never depends on old code.
  nsExec::ExecToLog /TIMEOUT=10000 'sc stop DoodleRayTunnelService'
  Sleep 1000
  nsExec::ExecToLog /TIMEOUT=10000 'sc stop DoodleRayTunnelService'
  nsExec::ExecToLog /TIMEOUT=10000 'sc delete DoodleRayTunnelService'
  Sleep 2000
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
  nsExec::ExecToLog /TIMEOUT=10000 'sc stop DoodleRayTunnelService'
  Sleep 1000
  nsExec::ExecToLog /TIMEOUT=10000 'sc stop DoodleRayTunnelService'
  nsExec::ExecToLog /TIMEOUT=10000 'sc delete DoodleRayTunnelService'
!macroend
