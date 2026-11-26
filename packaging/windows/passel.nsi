; Passel's Windows installer (NSIS 3), from the tree bundle.sh makes:
;
;   makensis -INPUTCHARSET UTF8 -DVERSION=0.2.0 -DDIST=<bundle dir> \
;     [-DVERSION_NUMERIC=0.2.0.0] [-DOUTFILE=<setup.exe>] \
;     packaging/windows/passel.nsi
;
; Installs for every user under Program Files, with a Start menu shortcut
; and an entry in Settings > Apps. Passel needs GnuPG: the installer
; offers to get Gpg4win when it does not find it.

Unicode true
SetCompressor /SOLID lzma
ManifestDPIAware true

!ifndef VERSION
  !error "VERSION is not defined (-DVERSION=x.y.z)"
!endif
!ifndef DIST
  !error "DIST is not defined (-DDIST=<bundle dir>)"
!endif
!ifndef OUTFILE
  !define OUTFILE "passel-${VERSION}-x86_64-setup.exe"
!endif

!define APP_NAME "Passel"
!define PUBLISHER "Pierre Sauvage"
!define HOMEPAGE "https://github.com/Pierrotws/passel"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Passel"
!define GPG4WIN_URL "https://gpg4win.org/download.html"
; The repository root: makensis works from this script's folder. DIST
; and OUTFILE are best given as absolute paths for the same reason.
!define ROOT "..\.."

Name "${APP_NAME}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${APP_NAME}"
InstallDirRegKey HKLM "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel admin
BrandingText "${APP_NAME} ${VERSION}"

; Details in the setup program's Properties. VIProductVersion takes four
; numbers only: VERSION may be 0.2.0+1a2b3c4d for an untagged build.
!ifndef VERSION_NUMERIC
  !define VERSION_NUMERIC "${VERSION}.0"
!endif
VIProductVersion "${VERSION_NUMERIC}"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP_NAME} installer"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "GPL-3.0-or-later"

!include "MUI2.nsh"
!include "x64.nsh"
!include "FileFunc.nsh"

!define MUI_ICON "${ROOT}\assets\icons\passel.ico"
!define MUI_UNICON "${ROOT}\assets\icons\passel.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_FUNCTION LaunchAsUser

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${ROOT}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; The app's languages (po/LINGUAS); the first is the default.
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "French"
!insertmacro MUI_LANGUAGE "German"
!insertmacro MUI_LANGUAGE "Spanish"
!insertmacro MUI_LANGUAGE "PortugueseBR"

LangString NoGnuPG ${LANG_ENGLISH} "Passel encrypts your passwords with GnuPG, which was not found on this computer.$\r$\n$\r$\nOpen the Gpg4win download page now?"
LangString NoGnuPG ${LANG_FRENCH} "Passel chiffre vos mots de passe avec GnuPG, qui est introuvable sur cet ordinateur.$\r$\n$\r$\nOuvrir maintenant la page de téléchargement de Gpg4win ?"
LangString NoGnuPG ${LANG_GERMAN} "Passel verschlüsselt Ihre Passwörter mit GnuPG, das auf diesem Computer nicht gefunden wurde.$\r$\n$\r$\nJetzt die Download-Seite von Gpg4win öffnen?"
LangString NoGnuPG ${LANG_SPANISH} "Passel cifra sus contraseñas con GnuPG, que no se encontró en este equipo.$\r$\n$\r$\n¿Abrir ahora la página de descarga de Gpg4win?"
LangString NoGnuPG ${LANG_PORTUGUESEBR} "O Passel criptografa suas senhas com o GnuPG, que não foi encontrado neste computador.$\r$\n$\r$\nAbrir agora a página de download do Gpg4win?"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "${APP_NAME} needs a 64-bit (x64) Windows."
    Abort
  ${EndIf}
  SetRegView 64
  ; Installed for every user: the shortcut goes to the shared Start menu.
  SetShellVarContext all
FunctionEnd

; The installer runs elevated; started through Explorer, Passel runs as
; the user instead, like from the Start menu.
Function LaunchAsUser
  Exec '"$WINDIR\explorer.exe" "$INSTDIR\bin\passel.exe"'
FunctionEnd

Section "Passel" SecMain
  SectionIn RO
  ; An update replaces the whole program: no DLL of an older GTK stays
  ; behind to be loaded by the new one.
  RMDir /r "$INSTDIR\bin"
  RMDir /r "$INSTDIR\lib"
  RMDir /r "$INSTDIR\share"

  SetOutPath "$INSTDIR"
  File /r "${DIST}\*.*"

  ; Started from the Start menu with bin\ as its folder, like GTK expects.
  SetOutPath "$INSTDIR\bin"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}.lnk" "$INSTDIR\bin\passel.exe" "" "$INSTDIR\bin\passel.exe" 0

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "URLInfoAbout" "${HOMEPAGE}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\bin\passel.exe"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1
  ; Settings > Apps shows the size, in KiB.
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "EstimatedSize" "$0"
SectionEnd

; gpgme finds GnuPG the way Gpg4win registers it.
Function .onInstSuccess
  ReadRegStr $0 HKLM "Software\GnuPG" "Install Directory"
  ${If} $0 == ""
    SetRegView 32
    ReadRegStr $0 HKLM "Software\GnuPG" "Install Directory"
    SetRegView 64
  ${EndIf}
  ${If} $0 == ""
  ${OrIfNot} ${FileExists} "$0\bin\gpgconf.exe"
    MessageBox MB_YESNO|MB_ICONINFORMATION "$(NoGnuPG)" /SD IDNO IDNO done
    ExecShell "open" "${GPG4WIN_URL}"
    done:
  ${EndIf}
FunctionEnd

Function un.onInit
  SetRegView 64
  SetShellVarContext all
FunctionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\${APP_NAME}.lnk"
  RMDir /r "$INSTDIR\bin"
  RMDir /r "$INSTDIR\lib"
  RMDir /r "$INSTDIR\share"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "${UNINSTALL_KEY}"
SectionEnd
