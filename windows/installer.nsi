; Custom NSIS installer template for the Windows build.
;
; This is a copy of the default template dx 0.7.9 ships (packages/cli/src/bundler/windows.rs,
; the NSIS_TEMPLATE const). We fork it for one reason: dx names everything after `product_name`,
; and it forces `product_name` to PascalCase of the crate name, so the Start Menu shortcut comes
; out as "CobaltManager" with no space and there's no config knob to change it. Here we hardcode
; the user-visible name to "Cobalt Manager" instead.
;
; What we changed from dx's default:
;   - the user-visible name is "Cobalt Manager" (window title, version ProductName, the Start Menu /
;     uninstall / Desktop shortcut names, and the Add/Remove Programs display name)
;   - a Components page with an optional Desktop shortcut, ticked by default
;   - Start Menu links to the Cobalt wiki (how to use it) and the Lythos wiki (how to make mods)
;   - an "Open Cobalt Manager" checkbox on the finish page
;   - the installer and uninstaller close a running Cobalt Manager first so locked files don't block them
; Everything else still uses dx's handlebars placeholders. The install DIRECTORY stays on the
; product_name placeholder (CobaltManager) on purpose so the on-disk path has no spaces.
;
; dx renders this file through handlebars with the same vars it feeds its own template. Keep the
; placeholder and if/each blocks intact, and never put literal double-brace tokens in the comments,
; handlebars parses the whole file including comment lines and an unmatched block breaks the build.
; If you bump the dx version, re-diff this against the new NSIS_TEMPLATE and re-apply the changes.

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"

; Basic installer attributes
Name "Cobalt Manager"
OutFile "{{output_path}}"
Unicode true
{{#if install_mode_per_machine}}
InstallDir "$PROGRAMFILES\{{product_name}}"
{{else}}
InstallDir "$LOCALAPPDATA\Programs\{{product_name}}"
{{/if}}

; Request appropriate privileges
{{#if install_mode_per_machine}}
RequestExecutionLevel admin
{{else if install_mode_both}}
RequestExecutionLevel admin
{{else}}
RequestExecutionLevel user
{{/if}}

; Version information
VIProductVersion "{{version}}.0"
VIAddVersionKey "ProductName" "Cobalt Manager"
VIAddVersionKey "FileVersion" "{{version}}"
VIAddVersionKey "ProductVersion" "{{version}}"
VIAddVersionKey "FileDescription" "{{short_description}}"
{{#if publisher}}
VIAddVersionKey "CompanyName" "{{publisher}}"
{{/if}}
{{#if copyright}}
VIAddVersionKey "LegalCopyright" "{{copyright}}"
{{/if}}

; MUI settings
!define MUI_ABORTWARNING
{{#if installer_icon}}
!define MUI_ICON "{{installer_icon}}"
{{/if}}
{{#if header_image}}
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "{{header_image}}"
{{/if}}
{{#if sidebar_image}}
!define MUI_WELCOMEFINISHPAGE_BITMAP "{{sidebar_image}}"
{{/if}}

; Pages
{{#if license}}
!insertmacro MUI_PAGE_LICENSE "{{license}}"
{{/if}}
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; Put an "Open Cobalt Manager" checkbox on the last page so the user can launch right after
; installing. It's ticked by default. The app inherits the installer's privileges, which is fine
; for our per-user install (RequestExecutionLevel user). If this ever becomes a per-machine (admin)
; install, launching from here would run the app as admin, which we'd want to avoid.
!define MUI_FINISHPAGE_RUN "$INSTDIR\{{main_binary_name}}"
!define MUI_FINISHPAGE_RUN_TEXT "Open Cobalt Manager"
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Language
!insertmacro MUI_LANGUAGE "English"
{{#each additional_languages}}
!insertmacro MUI_LANGUAGE "{{this}}"
{{/each}}

; Install components. The main one is required (SectionIn RO greys it out so it can't be unchecked).
; The Desktop shortcut is a separate component, ticked by default but the user can opt out on the
; components page (a plain Section is selected unless you mark it /o).
Section "Cobalt Manager" SecMain
    SectionIn RO

    ; Close the app first if it's already running, otherwise overwriting a locked .exe fails (matters
    ; on reinstall and on self-update). taskkill returns nonzero when nothing is running, we ignore it.
    nsExec::Exec 'taskkill /IM "{{main_binary_name}}" /F'
    Pop $0

    SetOutPath $INSTDIR

    ; Install main binary
    File "{{main_binary_path}}"

    ; Install resources
    {{#each staged_files}}
    SetOutPath "$INSTDIR{{#if this.target_dir}}\{{this.target_dir}}{{/if}}"
    File "{{this.source}}"
    {{/each}}

    SetOutPath $INSTDIR

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Create Start Menu shortcuts
    CreateDirectory "$SMPROGRAMS\{{start_menu_folder}}"
    CreateShortcut "$SMPROGRAMS\{{start_menu_folder}}\Cobalt Manager.lnk" "$INSTDIR\{{main_binary_name}}"
    CreateShortcut "$SMPROGRAMS\{{start_menu_folder}}\Uninstall Cobalt Manager.lnk" "$INSTDIR\uninstall.exe"

    ; Start Menu links to the wikis (.url files, they open in the browser). One for learning how to
    ; use Cobalt, one for making mods with Lythos. They sit in the same folder and are removed with
    ; it on uninstall.
    WriteINIStr "$SMPROGRAMS\{{start_menu_folder}}\How to use Cobalt (Wiki).url" "InternetShortcut" "URL" "https://github.com/Raytwo/Cobalt/wiki"
    WriteINIStr "$SMPROGRAMS\{{start_menu_folder}}\How to make mods (Lythos Wiki).url" "InternetShortcut" "URL" "https://github.com/DivineDragonFanClub/Lythos/wiki"

    ; Write registry keys for Add/Remove Programs
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "DisplayName" "Cobalt Manager"
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "DisplayVersion" "{{version}}"
    {{#if publisher}}
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "Publisher" "{{publisher}}"
    {{/if}}
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "InstallLocation" "$INSTDIR"

    ; Get installed size
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "EstimatedSize" "$0"

    {{#if install_webview}}
    ; WebView2 installation
    {{webview_install_code}}
    {{/if}}

SectionEnd

Section "Desktop shortcut" SecDesktop
    CreateShortcut "$DESKTOP\Cobalt Manager.lnk" "$INSTDIR\{{main_binary_name}}"
SectionEnd

; Text shown for each component when the user hovers it on the components page.
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
    !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} "Cobalt Manager and its Start Menu shortcuts."
    !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "Also add a shortcut on your Desktop."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

{{#if installer_hooks}}
!include "{{installer_hooks}}"
{{/if}}

; Uninstaller section
Section "Uninstall"
    ; Close the app if it's running so its files aren't locked while we delete them.
    nsExec::Exec 'taskkill /IM "{{main_binary_name}}" /F'
    Pop $0

    ; Remove files
    RMDir /r "$INSTDIR"

    ; Remove Start Menu items
    RMDir /r "$SMPROGRAMS\{{start_menu_folder}}"

    ; Remove Desktop shortcut
    Delete "$DESKTOP\Cobalt Manager.lnk"

    ; Remove registry keys
    DeleteRegKey SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}"
SectionEnd
