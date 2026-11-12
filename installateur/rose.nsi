; ═══════════════════════════════════════════════════════════════════════
;  Rose — installateur Windows x64 (HKCU, sans droits administrateur)
;  Installe le moteur, l'association .rose avec son icône et le PATH.
;  Construit par build-installer.sh via makensis (NSIS 3).
; ═══════════════════════════════════════════════════════════════════════
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "nsDialogs.nsh"

!define APP_NAME    "Rose"
!ifndef APP_VERSION
  !define APP_VERSION "3.14.0"
!endif
!define APP_PUBLISHER "Stef302"
!define EXE_NAME    "rose.exe"
!define UNINST_KEY  "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}"

Name "${APP_NAME} ${APP_VERSION}"
OutFile "Rose-Setup-${APP_VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\Rose"
InstallDirRegKey HKCU "Software\${APP_NAME}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ICON   "assets\setup.ico"
!define MUI_UNICON "assets\setup.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\bin\${EXE_NAME}"
!define MUI_FINISHPAGE_RUN_TEXT "Lancer « rose info » pour vérifier l'installation"
!define MUI_FINISHPAGE_SHOWREADME "$INSTDIR\README-PARTAGE.md"
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Lire README-PARTAGE.md"

!insertmacro MUI_PAGE_WELCOME
Page custom DetecterInstallation PreDetecter
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "French"

; ── aide : retire toutes les occurrences de $R1 dans $R0 (résultat $R0) ──
Var BinDir
Var Dialog
Var Radio1
Var Radio2
Var Radio3
Var ChoixAction

!macro CORPS_ENLEVER TAG
  StrLen $R2 "$R0"
  StrLen $R3 "$R1"
  StrCpy $R4 0
  StrCpy $R5 ""
  ${TAG}_boucle:
    StrCmp $R4 $R2 ${TAG}_fini
    StrCpy $R6 "$R0" $R3 $R4
    StrCmp $R6 "$R1" ${TAG}_saute
    StrCpy $R7 "$R0" 1 $R4
    StrCpy $R5 "$R5$R7"
    IntOp $R4 $R4 + 1
    Goto ${TAG}_boucle
  ${TAG}_saute:
    IntOp $R4 $R4 + $R3
    Goto ${TAG}_boucle
  ${TAG}_fini:
    StrCpy $R0 "$R5"
!macroend

Function Enlever
  !insertmacro CORPS_ENLEVER INST
FunctionEnd

Function un.Enlever
  !insertmacro CORPS_ENLEVER UNS
FunctionEnd

; ── PATH utilisateur : préfixe $INSTDIR\bin à l'installation, le retirer
# à la désinstallation, puis prévenir l'explorateur (WM_SETTINGCHANGE). ──
Function AjoutPath
  ReadRegStr $R0 HKCU "Environment" "Path"
  StrCpy $R1 "$BinDir;"
  Call Enlever
  StrCpy $R1 "$BinDir"
  Call Enlever
  StrCmp $R0 "" 0 avec_sep
    WriteRegExpandStr HKCU "Environment" "Path" "$BinDir"
    Goto diffuser
  avec_sep:
    WriteRegExpandStr HKCU "Environment" "Path" "$BinDir;$R0"
  diffuser:
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "UTF:Environment" /TIMEOUT=2000
FunctionEnd

Function un.RetraitPath
  ReadRegStr $R0 HKCU "Environment" "Path"
  StrCpy $R1 "$BinDir;"
  Call un.Enlever
  StrCpy $R1 "$BinDir"
  Call un.Enlever
  WriteRegExpandStr HKCU "Environment" "Path" "$R0"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "UTF:Environment" /TIMEOUT=2000
FunctionEnd

; ── détection d'une installation existante et choix intelligent ──────
Var InstallExistante

Function DetecterInstallation
  ReadRegStr $R0 HKCU "Software\${APP_NAME}" "InstallDir"
  IfErrors pas_trouve
  StrCmp $R0 "" pas_trouve
  IfFileExists "$R0\bin\${EXE_NAME}" trouve
  pas_trouve:
    Abort
  trouve:
    StrCpy $INSTDIR "$R0"
    nsDialogs::Create 1018
    Pop $Dialog
    ${If} $Dialog == error
      Abort
    ${EndIf}

    ${NSD_CreateLabel} 0 0u 100% 12u "Une installation de ${APP_NAME} ${APP_VERSION} a été détectée."
    Pop $R1
    ${NSD_CreateLabel} 0 14u 100% 12u "Dossier : $R0"
    Pop $R1
    ${NSD_CreateLabel} 0 30u 100% 12u "Que souhaitez-vous faire ?"
    Pop $R1

    ${NSD_CreateRadioButton} 10u 46u 100% 12u "Réinstaller — écraser tous les fichiers (recommandé)"
    Pop $Radio1
    ${NSD_CreateRadioButton} 10u 60u 100% 12u "Réparer — réinstaller uniquement les fichiers manquants"
    Pop $Radio2
    ${NSD_CreateRadioButton} 10u 74u 100% 12u "Désinstaller — supprimer Rose complètement"
    Pop $Radio3

    ${NSD_SetState} $Radio1 ${BST_CHECKED}
    StrCpy $ChoixAction "0"

    nsDialogs::Show

    ${NSD_GetState} $Radio1 $R1
    ${If} $R1 == ${BST_CHECKED}
      StrCpy $ChoixAction "0"
    ${EndIf}
    ${NSD_GetState} $Radio2 $R1
    ${If} $R1 == ${BST_CHECKED}
      StrCpy $ChoixAction "2"
    ${EndIf}
    ${NSD_GetState} $Radio3 $R1
    ${If} $R1 == ${BST_CHECKED}
      StrCpy $ChoixAction "1"
    ${EndIf}

    ${If} $ChoixAction == "1"
      IfFileExists "$R0\uninstall.exe" 0 pas_uninst
      ExecWait '"$R0\uninstall.exe" /S'
      pas_uninst:
      RMDir /r "$R0"
      DeleteRegKey HKCU "Software\${APP_NAME}"
      MessageBox MB_OK "Rose a été désinstallé. Relancez l'installeur pour réinstaller."
      Quit
    ${EndIf}
    ; 0 = réinstaller, 2 = réparer : dans les deux cas on copie les fichiers
    ; (NSIS écrase toujours ; la distinction est informative)
FunctionEnd

Function PreDetecter
  ; rien de spécial
FunctionEnd

Section "Moteur Rose" SecMoteur
  SectionIn RO
  StrCpy $BinDir "$INSTDIR\bin"

  CreateDirectory "$INSTDIR"
  CreateDirectory "$INSTDIR\bin"
  SetOutPath "$INSTDIR\bin"
  File "staging\rose-20.exe"                  ; → rose.exe
  Rename "rose-20.exe" "${EXE_NAME}"
  File "assets\rose.ico"                          ; icône des fichiers .rose
  File "assets\\setup.ico"                          ; icône de l'exécutable Rose
  Rename "setup.ico" "rose_app.ico"
  File /nonfatal "staging\LLVM-C.dll"             ; LLVM runtime (AOT/LLVM)


  SetOutPath "$INSTDIR"
  File /nonfatal "staging\README-PARTAGE.md"
  File /nonfatal "staging\TESTER.md"

  ; ── association .rose ────────────────────────────────────────────────
  WriteRegStr HKCU "Software\Classes\.rose" "" "Rose.File"
  WriteRegStr HKCU "Software\Classes\Rose.File" "" "Programme Rose"
  WriteRegStr HKCU "Software\Classes\Rose.File\DefaultIcon" "" "$INSTDIR\bin\rose.ico,0"
  WriteRegStr HKCU "Software\Classes\Rose.File\shell\open\command" "" \
              '"$INSTDIR\bin\${EXE_NAME}" "%1"'
  ; « rose » perçoit aussi le type pour l'éditeur de texte
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.rose\Shell\edit" "" "Modifier"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.rose\Shell\edit\command" "" \
              '%SystemRoot%\System32\notepad.exe "%1"'

  ; ── trace d'installation + désinstalleur ─────────────────────
  WriteRegStr HKCU "Software\${APP_NAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\${APP_NAME}" "Version" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\bin\rose_app.ico"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"

  ; rafraichir le cache d icones Windows
  Delete "$LOCALAPPDATA\IconCache.db"
  Delete "$LOCALAPPDATA\Microsoft\Windows\Explorer\iconcache_*.db"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  Call AjoutPath
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  StrCpy $BinDir "$INSTDIR\bin"
  Call un.RetraitPath

  ; rafraichir le cache d icones Windows
  Delete "$LOCALAPPDATA\IconCache.db"
  Delete "$LOCALAPPDATA\Microsoft\Windows\Explorer\iconcache_*.db"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  DeleteRegKey HKCU "Software\Classes\.rose"
  DeleteRegKey HKCU "Software\Classes\Rose.File"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.rose"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\${APP_NAME}"

  Delete "$INSTDIR\bin\LLVM-C.dll"
  RMDir /r "$INSTDIR\bin"
  Delete "$INSTDIR\README-PARTAGE.md"
  Delete "$INSTDIR\TESTER.md"
  Delete "$INSTDIR\README.txt"
  RMDir /r "$INSTDIR\exemples"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
