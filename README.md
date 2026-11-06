# MPK Deck

MPK Deck es una aplicación para Windows x64 que usa el Akai MPK Mini MK3 para disparar clips, ajustar volúmenes y ejecutar macros asociadas a perfiles. La versión 0.3.0 añade un mapa interactivo del controlador para configurar las asignaciones MIDI.

## Primer uso

1. Abre **Conexiones**, selecciona el puerto de entrada del MPK y una salida local de preescucha. Pulsa **Guardar cambios**.
2. En **Soundboard**, importa un WAV o MP3. Se admiten archivos mono o estéreo, de 8 a 96 kHz, hasta 30 segundos y 24 MiB. Pulsa **Probar** para usar la salida local.
3. En **Asignaciones MIDI**, elige un control del dibujo del MPK. En el panel que se abre junto a él, pulsa **Aprender** y usa el control físico. Configura la acción, pulsa **Aplicar al perfil** y después **Guardar cambios**. Para los pads, selecciona el mismo banco en el MPK y en el dibujo. Las asignaciones anteriores siguen disponibles en **Asignación avanzada** y se pueden vincular al mapa aprendiendo el mismo mensaje. Para comprobar el puerto o los mensajes, activa **Mostrar eventos**.
4. Para volumen continuo, elige un control CC, modo **Continuo (CC)**, la acción **Ajustar volumen** y sus límites. La aplicación aparece en el mezclador cuando Windows tiene alguna sesión de audio identificable.
   Para controlar la salida predeterminada de Windows, elige **Ajustar volumen general** y configura sus límites. Si cambias la salida predeterminada, el siguiente movimiento del mando controla la nueva salida.
5. En **Macros**, configura hasta 16 pasos y un máximo acumulado de 10 segundos de espera. Opcionalmente exige una aplicación en primer plano. Una macro termina al fijar un perfil y se cancela al fallar un paso, cambiar de perfil, pausar macros o pulsar **Detener**. El contexto se comprueba al activar la macro y antes de cada paso.
6. En **Perfiles**, activa la automatización si quieres que se apliquen reglas por aplicación en primer plano. La prioridad más alta gana y los empates siguen el orden de la lista. La demora se aplica también al volver al perfil de respaldo. **Fijar este perfil al guardar** desactiva la automatización.

Cada clip admite una repetición hasta detenerlo. Los modos **Mientras mantienes** y **Alternante** responden al flanco de entrada y controlan la voz de esa asignación. Los CC momentáneos deben enviar cero al soltar. Hay cuatro voces por salida y se reserva margen fijo por voz para la mezcla. La salida usa el formato compartido elegido por Windows. La latencia de entrada a salida queda por medir.

## Micrófono MPK Deck para Discord

Con el controlador de MPK Deck instalado, puedes combinar tu micrófono y los clips dentro de la aplicación.

1. En **Conexiones**, selecciona tus auriculares como preescucha. En **Micrófono MPK Deck**, pulsa **Usar salida de MPK Deck**, elige tu micrófono físico y guarda.
2. Pulsa **Activar micrófono**. Cada inicio requiere activación manual. **Desactivar micrófono** corta la captura y **Detener clips** detiene los sonidos. La preescucha se limita a los clips.
3. En Discord, selecciona **MPK Deck Microphone (MPK Deck Virtual Audio)** o la entrada equivalente de ese adaptador como micrófono. Mantén los auriculares como salida. Prueba hablando y pulsando un pad cuya acción reproduzca por **Salida para Discord**.

La compilación de desarrollo trae la mezcla, pero el controlador espera firma de Microsoft y validación con Discord. La app muestra cuando Windows no detecta los endpoints. Consulta la [compilación, firma y empaquetado del controlador](driver/README.md).

## Enviar clips a Discord con una salida virtual externa

MPK Deck necesita una salida de reproducción conectada a una entrada virtual que Discord pueda seleccionar. Una opción para una instalación personal es **VB-CABLE Virtual Audio Device**, de VB-Audio Software, distribuido como donationware. La app acepta el endpoint de reproducción expuesto por Windows. El controlador y su versión quedan a cargo del usuario. El flujo con VB-CABLE y Discord queda por verificar.

1. Descarga el paquete x64 para Windows del [sitio oficial de VB-Audio](https://vb-audio.com/Cable/). Comprueba la procedencia y la firma del instalador antes de ejecutarlo.
2. Instala el controlador siguiendo las instrucciones del proveedor. El instalador de VB-Audio requiere administrador y un reinicio para completar el alta. MPK Deck se instala por usuario.
3. En MPK Deck, selecciona **CABLE Input** en **Salida para Discord** y tus auriculares en **Salida de preescucha**. Guarda. En Discord, elige **CABLE Output** como dispositivo de entrada y tus auriculares como salida. Haz una prueba de micrófono con un clip.
4. VB-CABLE por sí solo transporta los clips. Para conservar el micrófono físico a la vez, usa una mezcla externa de micrófono y clips con una entrada virtual dedicada. Mantén desactivada la escucha de esa entrada en Windows y evita enviar el audio de Discord al mismo cable.
5. Si desaparece un endpoint, reconecta el dispositivo o reinicia el servicio de audio. Vuelve a elegir la salida si cambió su nombre. El botón **Probar** sigue disponible con una ruta local válida. Para retirar VB-CABLE, usa la opción de desinstalación del proveedor con privilegios de administrador y reinicia. Selecciona de nuevo tu micrófono en Discord.

Consulta el [manual de VB-CABLE](https://vb-audio.com/Cable/VBCABLE_ReferenceManual.pdf) para requisitos, recuperación y detalles de formato.

## Desarrollo y compilación

Requisitos de la compilación comprobada en Windows x64:

- Rust **1.99.1**, destino **x86_64-pc-windows-msvc**, fijado en `rust-toolchain.toml`.
- Node **22.21.1** y npm **10.9.4**. Usa `npm ci` con el lockfile.
- Visual Studio Build Tools 2022 **17.14**, carga **Desktop development with C++** y Windows SDK **10.0.26100.0**.
- WebView2 Evergreen instalado. Tauri CLI **2.12.4** y las dependencias Rust se resuelven con los lockfiles.

En PowerShell, desde la raíz:

```powershell
npm ci
npm run tauri dev
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run tauri build -- --ci --bundles nsis -- --locked
```

El instalador NSIS x64 en español e inglés se genera en `src-tauri/target/release/bundle/nsis/`. Los binarios de desarrollo se generan sin firma. Para publicar, incorpora la firma del editor y verifica instalación, actualización, desinstalación y conservación de datos en una máquina limpia. Consulta los [prerrequisitos oficiales de Tauri](https://v2.tauri.app/start/prerequisites/) y su [guía de instaladores de Windows](https://v2.tauri.app/distribute/windows-installer/).

El instalador incluye los avisos de dependencias. Después de actualizar los lockfiles, vuelve a generarlos con `./scripts/third-party-notices.ps1` en PowerShell 7 y revisa las nuevas licencias. No se incorporan controladores de audio de terceros.

## Licencia

El código original de MPK Deck se publica bajo la [PolyForm Noncommercial License 1.0.0](LICENSE.md). Consulta el [aviso de titularidad](NOTICE). Para usos comerciales fuera de los supuestos que permite la licencia, solicita autorización a [diegoblo](https://github.com/diegoblo).

El [parche derivado del controlador de Microsoft](driver/sysvad-voice.patch) se distribuye bajo la [Microsoft Public License](driver/LICENSE-MS-PL.txt). Las dependencias se rigen por sus [licencias y avisos respectivos](src-tauri/resources/third-party-notices.txt).

## Datos y recuperación

El directorio de datos de la aplicación es `%LOCALAPPDATA%\app.mpkdeck.desktop`. Para respaldarlo, cierra la app y copia la carpeta completa. Contiene `settings.json`, la última copia válida `settings.backup.json` y la biblioteca `sounds`. Para restaurar, cierra la app y recupera la carpeta desde tu copia.

Si el archivo principal es ilegible se intenta abrir la copia válida. Al guardar, el archivo dañado se conserva como `settings-recovered-*.json`. El formato actual es v1. Quitar un clip del catálogo conserva el archivo en la carpeta de datos para poder restaurar una copia anterior.

**Exportar JSON** guarda la definición del perfil, incluidas macros, asignaciones y reglas. Las referencias a clips y otros perfiles requieren sus identificadores en la instalación de destino. Para migrar la biblioteca y los perfiles juntos, respalda la carpeta completa. Las rutas de ejecutables guardadas para reglas y volumen pueden necesitar volver a seleccionarse en otro equipo. El diagnóstico MIDI solo se mantiene en memoria mientras está visible y no registra texto escrito.
