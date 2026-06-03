!include LogicLib.nsh

!macro DoodleRayExecOrAbort COMMAND LABEL
  DetailPrint "${LABEL}"
  nsExec::ExecToStack '${COMMAND}'
  Pop $0
  Pop $1
  DetailPrint "$1"
  ${If} $0 != 0
    MessageBox MB_ICONSTOP|MB_OK "${LABEL} failed.$\r$\n$\r$\n$1"
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Preparing DoodleRay Tunnel Service for update..."
  ; Do not call the previously installed DoodleRayService.exe here: older
  ; service builds used a different pipe/protocol and can block the updater.
  ; The app calls PrepareForUpdate before launching the updater; this hook is a
  ; last-resort SCM cleanup so installer replacement never depends on old code.
  nsExec::ExecToLog 'sc stop DoodleRayTunnelService'
  Sleep 1000
  nsExec::ExecToLog 'sc stop DoodleRayTunnelService'
  nsExec::ExecToLog 'sc delete DoodleRayTunnelService'
  Sleep 2000
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro DoodleRayExecOrAbort '"$INSTDIR\DoodleRayService.exe" install' "Installing DoodleRay Tunnel Service"
  Sleep 1000
  !insertmacro DoodleRayExecOrAbort '"$INSTDIR\DoodleRayService.exe" status' "Checking DoodleRay Tunnel Service"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Removing DoodleRay Tunnel Service..."
  nsExec::ExecToLog 'sc stop DoodleRayTunnelService'
  Sleep 1000
  nsExec::ExecToLog 'sc stop DoodleRayTunnelService'
  nsExec::ExecToLog 'sc delete DoodleRayTunnelService'
!macroend
