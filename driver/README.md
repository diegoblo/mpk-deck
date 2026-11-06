# Micrófono virtual de MPK Deck

El controlador es un derivado del [Simple Audio Sample de Microsoft](https://github.com/microsoft/Windows-driver-samples/tree/f4c1ff842b6a0001152f44cb61788b216a1c7627/audio/simpleaudiosample), con el parche y el enlace WaveRT de esta carpeta. El código derivado conserva los avisos de Microsoft y la licencia MS-PL. El script obtiene esa revisión exacta y aplica el parche en un directorio de compilación nuevo.

- Identificador PnP `ROOT\MPKDeckVoice`. Adaptador `MPK Deck Virtual Audio`. El nombre visible en Windows combina el nombre del endpoint y el del adaptador.
- Entrada y salida WaveRT x64, PCM estéreo de 16 bits a 48 kHz, con destino mínimo Windows 10 2004 y KMDF 1.31. La ruta física del micrófono se abre en el formato compartido de Windows y Rodio adapta la mezcla al endpoint.
- El enlace del controlador mantiene hasta 60 ms de PCM por adaptador, descarta lo más antiguo al llenarse y entrega silencio si le faltan muestras. Vacía la cola al cambiar el estado de los flujos y al bloquear contenido protegido. La captura de la app se mantiene en memoria y se detiene al cerrar, cambiar la configuración o perder un dispositivo.
- Ambos endpoints virtuales están marcados para que Windows no los elija como dispositivo predeterminado. Selecciona explícitamente el micrófono virtual en Discord.

## Compilar el controlador de desarrollo

Instala Visual Studio 2022 Build Tools con MSVC x64, el SDK **10.0.26100.0** y el WDK **10.0.26100.6584**. Desde PowerShell 7.4 o superior, ejecuta:

```powershell
./driver/build.ps1
```

Genera `MPKDeckVoice.sys`, `MPKDeckVoice.inf`, `MPKDeckVoice.cat` y la licencia en el directorio `unsigned` indicado al final. Inf2Cat valida el paquete para Windows 10 2004 y Windows 11 x64. El script genera también `MPKDeckVoice-x64.cab` con el INF, SYS, CAT y los símbolos PDB para una [solicitud de atestación](https://learn.microsoft.com/windows-hardware/drivers/dashboard/code-signing-attestation). El CAB requiere la firma del editor antes del envío. Windows usa los archivos firmados que devuelve Microsoft para instalar el controlador.

## Preparar el instalador con el controlador

Envía el paquete a través del proceso de [firma y certificación de controladores de Microsoft](https://learn.microsoft.com/windows-hardware/drivers/install/windows-driver-signing-tutorial). Después, coloca el INF, SYS y CAT devueltos por Microsoft en una carpeta y ejecuta:

```powershell
./driver/build-signed-installer.ps1 -SignedPackage "C:\ruta\al\paquete-firmado"
```

El script valida la firma bajo la política de kernel de Windows y comprueba que INF y SYS pertenecen al catálogo. Genera un instalador NSIS separado para todos los usuarios. El asistente solicita permiso explícito para registrar el controlador en la instalación interactiva en Archivos de programa. En una instalación silenciosa se instala la app. El desinstalador retira el dispositivo y su paquete PnP, con aviso si es necesario reiniciar o la retirada falla. La firma del instalador de MPK Deck es un paso de publicación separado de la firma del controlador.

La edición de desarrollo instalada por usuario puede coexistir con la edición para todos los usuarios. Antes de migrar de edición, respalda los datos, cierra MPK Deck y desinstala la edición de desarrollo conservando los datos del usuario.

## Validación

Comprobado en Windows 10 x64, compilación 19045, con VS Build Tools 17.14 y WDK 26100.6584. El paquete x64 y el catálogo se generaron sin errores ni advertencias de Inf2Cat. SignTool rechazó el catálogo sin firma. Se compiló el registrador y se comprobó que rechaza una instalación sin INF. La lógica de la cola pasó una prueba en modo usuario de desbordamiento, silencio, parada y bloqueo de contenido protegido. Esa prueba no mide IRQL, concurrencia real, ni latencia.

Para repetir la prueba de la cola, desde Developer PowerShell x64, en la raíz:

```powershell
New-Item -ItemType Directory -Force driver/out | Out-Null
cl /nologo /std:c++17 /EHsc /W4 /WX /Fo:driver/out/bridge-test.obj /Fe:driver/out/bridge-test.exe driver/voice-bridge-test.cpp
./driver/out/bridge-test.exe
```

Quedan por verificar con un paquete firmado la instalación limpia y la actualización, el arranque de ambos endpoints, el recorrido de voz y clips hasta Discord, los niveles, latencia, recuperación ante suspensión y la desinstalación. Usa una máquina de pruebas o VM para validar el controlador con las herramientas del WDK antes de distribuir.

Con el controlador instalado y Discord cerrado, puedes comprobar que un clip enviado a la salida reaparece en el micrófono virtual:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml transfers_tone_through_virtual_microphone -- --ignored --nocapture
```
