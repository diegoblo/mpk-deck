; The per-machine installer has already requested elevation. The app uses an
; unprivileged process and a separate user-data directory.
!macro NSIS_HOOK_POSTINSTALL
  ${IfNot} ${Silent}
    MessageBox MB_YESNO|MB_DEFBUTTON2|MB_ICONQUESTION "Install the MPK Deck virtual audio driver for microphone and clips in Discord? / ¿Instalar el controlador de audio virtual de MPK Deck para el micrófono y los clips en Discord?" IDNO mpk_voice_skip
    ${If} $INSTDIR != "$PROGRAMFILES64\MPK Deck"
      MessageBox MB_ICONEXCLAMATION "Install MPK Deck in Program Files to enable virtual audio. / Instala MPK Deck en Archivos de programa para habilitar el audio virtual."
      Goto mpk_voice_skip
    ${EndIf}
    ClearErrors
    ExecWait '"$INSTDIR\voice-driver\mpk-voice-register.exe" install' $0
    ${If} ${Errors}
      StrCpy $0 4
    ${EndIf}
    ${If} $0 == 3010
      SetRebootFlag true
    ${ElseIf} $0 != 0
      MessageBox MB_ICONEXCLAMATION "MPK Deck was installed. Virtual audio driver installation failed ($0). / MPK Deck se instaló. Falló la instalación del controlador de audio virtual ($0)."
    ${EndIf}
  ${EndIf}
  mpk_voice_skip:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
  ${AndIf} $INSTDIR == "$PROGRAMFILES64\MPK Deck"
    ${If} ${FileExists} "$INSTDIR\voice-driver\mpk-voice-register.exe"
      ClearErrors
      ExecWait '"$INSTDIR\voice-driver\mpk-voice-register.exe" remove' $0
      ${If} ${Errors}
        StrCpy $0 4
      ${EndIf}
      ${If} $0 == 3010
        SetRebootFlag true
      ${ElseIf} $0 != 0
        DetailPrint "Virtual audio driver removal returned $0. / La retirada del controlador devolvió $0."
        ${IfNot} ${Silent}
          MessageBox MB_ICONEXCLAMATION "Check the MPK Deck device in Device Manager ($0). / Revisa el dispositivo MPK Deck en Administrador de dispositivos ($0)."
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
