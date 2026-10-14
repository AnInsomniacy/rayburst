; The scoped hooks below replace Tauri's basename-only process check, which
; could terminate a different Rayburst installation.
!macroundef CheckIfAppIsRunning
!macro CheckIfAppIsRunning executableName productName
!macroend

; Run the new executable from NSIS-owned temporary storage. No installed file
; is replaced until the exact installation's processes have exited.
!macro RAYBURST_PREPARE_INSTALL
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File /oname=rayburst-maintenance.exe "${MAINBINARYSRCPATH}"
  nsExec::ExecToStack '"$PLUGINSDIR\rayburst-maintenance.exe" --prepare-install "$INSTDIR"'
  Pop $R0
  Pop $R1
  SetOutPath "$INSTDIR"
  ${If} $R0 != 0
    DetailPrint "$R1"
    Abort "Rayburst could not stop the installed processes. $R1"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro RAYBURST_PREPARE_INSTALL
!macroend

; Tauri may restore the same ProgID from a previous installation during uninstall.
; Do not leave a default pointing at the application class we just removed.
!macro NSIS_HOOK_POSTUNINSTALL
  ReadRegStr $R0 SHCTX "Software\Classes\.torrent" ""
  ${If} $R0 == "${BUNDLEID}.torrent"
    DeleteRegValue SHCTX "Software\Classes\.torrent" ""
  ${EndIf}
  DeleteRegValue SHCTX "Software\Classes\.torrent" "${BUNDLEID}.torrent_backup"
  ; Tauri removes the login Run value; its Task Manager switch goes with it.
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
  ${EndIf}
!macroend

; Rayburst Native Messaging registration and icon refresh.

; Advertise capabilities without taking over the user's public defaults.
!macro RAYBURST_REGISTER_CANDIDATE suffix association section
  !if "${section}" == "URLAssociations"
    WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.${suffix}" "URL Protocol" ""
  !endif
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.${suffix}" "" "Rayburst ${association}"
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.${suffix}\DefaultIcon" "" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.${suffix}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
  WriteRegStr SHCTX "Software\${BUNDLEID}\Capabilities\${section}" "${association}" "${BUNDLEID}.${suffix}"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "Software\${BUNDLEID}\Capabilities" "ApplicationName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "Software\${BUNDLEID}\Capabilities" "ApplicationDescription" "Download files and media with Rayburst"
  WriteRegStr SHCTX "Software\${BUNDLEID}\Capabilities" "ApplicationIcon" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr SHCTX "Software\RegisteredApplications" "${BUNDLEID}" "Software\${BUNDLEID}\Capabilities"
  !insertmacro RAYBURST_REGISTER_CANDIDATE torrent .torrent FileAssociations
  WriteRegStr SHCTX "Software\Classes\.torrent\OpenWithProgids" "${BUNDLEID}.torrent" ""
  !insertmacro RAYBURST_REGISTER_CANDIDATE magnet magnet URLAssociations
  !insertmacro RAYBURST_REGISTER_CANDIDATE ed2k ed2k URLAssociations
  !insertmacro RAYBURST_REGISTER_CANDIDATE thunder thunder URLAssociations
  !insertmacro RAYBURST_REGISTER_CANDIDATE rayburst rayburst URLAssociations
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  ; Register the allowlisted, activation-only native messaging host.
  WriteRegStr SHCTX \
    "Software\Google\Chrome\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" \
    "" "$INSTDIR\native-messaging\manifests\chromium.json"
  WriteRegStr SHCTX \
    "Software\Microsoft\Edge\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" \
    "" "$INSTDIR\native-messaging\manifests\chromium.json"
  WriteRegStr SHCTX \
    "Software\Mozilla\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" \
    "" "$INSTDIR\native-messaging\manifests\firefox.json"

  ; Flush Windows icon cache so updated icons appear immediately.
  ; ie4uinit.exe is a built-in Windows 10/11 system utility that
  ; soft-refreshes the shell icon display without requiring a reboot.
  ; This is the industry-standard approach used by Electron, VS Code,
  ; and other major desktop applications.
  nsExec::ExecToLog 'ie4uinit.exe -show'
!macroend

!macro RAYBURST_REMOVE_PROTOCOL scheme
  ReadRegStr $R0 HKCU "Software\Classes\${scheme}\shell\open\command" ""
  ${If} $R0 == '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
    DeleteRegKey HKCU "Software\Classes\${scheme}"
  ${EndIf}
  ReadRegStr $R0 HKCU "Software\Classes\${BUNDLEID}.${scheme}\shell\open\command" ""
  ${If} $R0 == '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
    DeleteRegKey HKCU "Software\Classes\${BUNDLEID}.${scheme}"
    DeleteRegValue HKCU "Software\${BUNDLEID}\Capabilities\URLAssociations" "${scheme}"
  ${EndIf}
!macroend

!macro RAYBURST_REMOVE_CANDIDATE hive suffix
  ReadRegStr $R0 ${hive} "Software\Classes\${BUNDLEID}.${suffix}\shell\open\command" ""
  ${If} $R0 == '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
    !if "${suffix}" == "torrent"
      DeleteRegValue ${hive} "Software\Classes\.torrent\OpenWithProgids" "${BUNDLEID}.torrent"
      ReadRegStr $R1 ${hive} "Software\Classes\.torrent" ""
      ${If} $R1 == "${BUNDLEID}.torrent"
        DeleteRegValue ${hive} "Software\Classes\.torrent" ""
      ${EndIf}
    !endif
    DeleteRegKey ${hive} "Software\Classes\${BUNDLEID}.${suffix}"
  ${EndIf}
!macroend

!macro RAYBURST_REMOVE_CAPABILITIES hive
  !insertmacro RAYBURST_REMOVE_CANDIDATE ${hive} torrent
  !insertmacro RAYBURST_REMOVE_CANDIDATE ${hive} magnet
  !insertmacro RAYBURST_REMOVE_CANDIDATE ${hive} ed2k
  !insertmacro RAYBURST_REMOVE_CANDIDATE ${hive} thunder
  !insertmacro RAYBURST_REMOVE_CANDIDATE ${hive} rayburst
  ReadRegStr $R0 ${hive} "Software\${BUNDLEID}\Capabilities" "ApplicationIcon"
  ${If} $R0 == '"$INSTDIR\${MAINBINARYNAME}.exe",0'
    DeleteRegKey ${hive} "Software\${BUNDLEID}\Capabilities"
    DeleteRegValue ${hive} "Software\RegisteredApplications" "${BUNDLEID}"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro RAYBURST_PREPARE_INSTALL
  !insertmacro RAYBURST_REMOVE_PROTOCOL rayburst
  !insertmacro RAYBURST_REMOVE_PROTOCOL magnet
  !insertmacro RAYBURST_REMOVE_PROTOCOL ed2k
  !insertmacro RAYBURST_REMOVE_PROTOCOL thunder
  !insertmacro RAYBURST_REMOVE_CAPABILITIES SHCTX
  !insertmacro RAYBURST_REMOVE_CAPABILITIES HKCU
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  ; Remove only registrations that still belong to this installation.
  ReadRegStr $R0 SHCTX \
    "Software\Google\Chrome\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" ""
  ${If} $R0 == "$INSTDIR\native-messaging\manifests\chromium.json"
    DeleteRegKey SHCTX \
      "Software\Google\Chrome\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser"
  ${EndIf}
  ReadRegStr $R0 SHCTX \
    "Software\Microsoft\Edge\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" ""
  ${If} $R0 == "$INSTDIR\native-messaging\manifests\chromium.json"
    DeleteRegKey SHCTX \
      "Software\Microsoft\Edge\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser"
  ${EndIf}
  ReadRegStr $R0 SHCTX \
    "Software\Mozilla\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" ""
  ${If} $R0 == "$INSTDIR\native-messaging\manifests\firefox.json"
    DeleteRegKey SHCTX \
      "Software\Mozilla\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser"
  ${EndIf}

  ; Runtime repair writes HKCU even for per-machine installs.
  ReadRegStr $R0 HKCU \
    "Software\Google\Chrome\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" ""
  ${If} $R0 == "$INSTDIR\native-messaging\manifests\chromium.json"
    DeleteRegKey HKCU \
      "Software\Google\Chrome\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser"
  ${EndIf}
  ReadRegStr $R0 HKCU \
    "Software\Microsoft\Edge\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" ""
  ${If} $R0 == "$INSTDIR\native-messaging\manifests\chromium.json"
    DeleteRegKey HKCU \
      "Software\Microsoft\Edge\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser"
  ${EndIf}
  ReadRegStr $R0 HKCU \
    "Software\Mozilla\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser" ""
  ${If} $R0 == "$INSTDIR\native-messaging\manifests\firefox.json"
    DeleteRegKey HKCU \
      "Software\Mozilla\NativeMessagingHosts\dev.aninsomniacy.rayburst.browser"
  ${EndIf}
!macroend
