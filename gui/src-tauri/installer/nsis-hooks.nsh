; Presentation of the Windows installer. Tauri includes this file after MUI2.nsh
; and before it declares any page, so these settings reach its stock wizard
; without forking the template. Keep this file ASCII: NSIS reads it in the build
; machine's code page.

; The system font of current Windows, in place of 8-point MS Shell Dlg. Pages are
; laid out in dialog units, so the whole wizard grows with it.
SetFont "Segoe UI" 9

; Go straight from the progress bar to the finish page. A failed install stops
; on the progress page, so its details stay on screen.
AutoCloseWindow true

!define MUI_HEADERIMAGE_RIGHT

!define MUI_WELCOMEPAGE_TITLE "Welcome to Pipeline"
!define MUI_WELCOMEPAGE_TEXT "Review papers, work on research projects with an AI assistant, and automate writing and review tasks.$\r$\n$\r$\nClick Next to continue."

!define MUI_FINISHPAGE_TITLE "Pipeline is ready"
!define MUI_FINISHPAGE_TEXT "Pipeline has been installed on your computer."
