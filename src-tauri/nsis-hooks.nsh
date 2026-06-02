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
  nsExec::ExecToLog '"$INSTDIR\DoodleRayService.exe" prepare-update'
  nsExec::ExecToLog '"$INSTDIR\DoodleRayService.exe" uninstall'
  nsExec::ExecToLog 'sc stop DoodleRayTunnelService'
  nsExec::ExecToLog 'sc delete DoodleRayTunnelService'
  Sleep 1500
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro DoodleRayExecOrAbort '"$INSTDIR\DoodleRayService.exe" install' "Installing DoodleRay Tunnel Service"
  Sleep 1000
  !insertmacro DoodleRayExecOrAbort '"$INSTDIR\DoodleRayService.exe" status' "Checking DoodleRay Tunnel Service"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Removing DoodleRay Tunnel Service..."
  nsExec::ExecToLog '"$INSTDIR\DoodleRayService.exe" prepare-update'
  nsExec::ExecToLog '"$INSTDIR\DoodleRayService.exe" uninstall'
!macroend
