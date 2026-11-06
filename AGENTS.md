# AGENTS.md — MPK Deck

## Propósito

Construye MPK Deck como una aplicación de escritorio para Windows que convierte un Akai MPK Mini MK3 en una superficie de control fiable para Discord: dispara sonidos, controla audio por aplicación, ejecuta macros contextuales y cambia perfiles automáticamente. La aplicación usa Rust y Tauri 2 en el backend y React + TypeScript en la interfaz. Prioriza estabilidad durante llamadas, baja latencia, recuperación clara ante fallos y una experiencia que se pueda configurar sin conocimientos de MIDI.

Este archivo es la guía operativa del repositorio. Antes de cambiar código, inspecciona su estructura, `README`, manifiestos, configuración de Tauri, decisiones de arquitectura y estado de Git. Conserva las convenciones y cambios locales existentes. No presupongas que el repositorio está vacío ni reemplaces componentes funcionales sin entenderlos.

## Principios de trabajo para Codex

- Empieza por entender el flujo completo que vas a tocar. Busca implementaciones y dependencias existentes antes de añadir código o paquetes.
- Reutiliza código existente cuando sea correcto para el caso de uso, esté mantenido, tenga licencia compatible y no arrastre una superficie de riesgo o mantenimiento desproporcionada. Reutilizar no significa copiar una aplicación entera ni heredar sus defectos.
- Antes de adoptar código externo, revisa licencia, actividad y compatibilidad con Windows, Rust, Tauri 2 y la arquitectura del proyecto. Conserva avisos requeridos y documenta atribuciones. No copies código de licencia desconocida ni elimines avisos.
- Prefiere APIs pequeñas y tipos explícitos. Mantén el acceso a hardware, audio, configuración del sistema y procesos en Rust. React presenta estado y envía intenciones mediante comandos tipados.
- No introduzcas dependencias por conveniencia si la biblioteca estándar o una dependencia existente resuelve la necesidad. Justifica dependencias con impacto en tamaño, mantenimiento, permisos, licencia y distribución.
- No ocultes fallos con valores predeterminados silenciosos. Devuelve errores estructurados y mensajes útiles, registra contexto técnico sin datos privados y ofrece una acción de recuperación cuando sea posible.
- Mantén los cambios acotados, coherentes con el estilo local y accesibles. Actualiza documentación, configuración y migraciones junto con el código.
- No afirmes que algo está probado en hardware, Discord, un dispositivo virtual o Windows si no se ejecutó en ese entorno. Separa con claridad las pruebas automatizadas de las verificaciones manuales.

## Arquitectura esperada

Mantén límites claros entre estas áreas; adapta los nombres a la estructura existente antes de crear módulos nuevos:

1. **Interfaz React + TypeScript**: configuración de dispositivos y asignaciones, soundboard, controles de mezcla, macros, perfiles, estado de conexión y diagnóstico. No accede directamente al sistema operativo.
2. **Capa de comandos Tauri**: comandos y eventos tipados, validación de entrada, errores serializables y permisos mínimos. No expongas una API genérica para ejecutar comandos del sistema.
3. **Servicios Rust**: MIDI, audio, sesiones de volumen de Windows, macros, perfiles, persistencia y diagnóstico. Mantén cada servicio independiente de la UI y define interfaces sustituibles para pruebas.
4. **Adaptadores del sistema**: encapsulan APIs de Windows, controladores, dispositivos y procesos. Aíslan diferencias de plataforma y versiones.
5. **Persistencia**: configuración versionada, con migraciones explícitas y recuperación frente a archivos incompletos o inválidos. Escribe de forma atómica cuando corresponda y conserva una copia recuperable antes de migraciones destructivas.

Usa eventos para notificar cambios asíncronos como conexión MIDI, estado de audio y perfil activo. Evita sondeos rápidos innecesarios, bloqueos del hilo UI, tareas sin cancelación y estado duplicado entre React y Rust. Define quién es dueño de cada estado y cómo se reconcilia tras reinicios o desconexiones.

## MIDI — Akai MPK Mini MK3

- Inspecciona los mensajes MIDI reales del MK3 y permite aprender una asignación desde la entrada del dispositivo. No codifiques números de nota, canal o controlador como si fueran universales.
- Soporta las entradas relevantes: notas, note-off, velocidad, canal y mensajes de control continuo. Filtra mensajes irrelevantes y maneja correctamente note-on con velocidad cero como note-off cuando aplique.
- Permite elegir el dispositivo explícitamente, mostrar su conexión y reconexión y evitar suscripciones duplicadas. Si el dispositivo desaparece, conserva la configuración y comunica el estado sin cerrar inesperadamente la aplicación.
- Distingue controles momentáneos, alternantes y continuos; ofrece límites, sensibilidad o curva para controles continuos cuando el diseño lo requiera. Evita disparar una misma acción dos veces por eventos de liberación o rebote.
- Evita bloquear el hilo de audio o UI al leer MIDI. Define colas acotadas, cancelación y limpieza al cerrar. La latencia y el comportamiento deben medirse, no prometerse sin mediciones.
- Proporciona una pantalla de diagnóstico que muestre mensajes entrantes, canal, valor y asignación resultante, con una opción de ocultar datos y limpiar el registro.

## Soundboard y micrófono virtual para Discord

- Trata el soundboard como un motor de reproducción separado de la UI: carga y valida formatos admitidos, limita duración y tamaño, gestiona voces concurrentes, stop, pausa si se admite, repetición y niveles sin producir clipping. Maneja archivos borrados o inaccesibles.
- Para enviar audio a Discord, diseña una salida seleccionable que funcione con un dispositivo de entrada virtual de Windows. No supongas que Tauri o Rust crean por sí solos un micrófono virtual. Investiga los requisitos de controlador, firma, instalación, privilegios, compatibilidad y desinstalación antes de elegir una solución.
- No instales ni registres controladores de forma silenciosa. Si la arquitectura depende de un controlador virtual de terceros, documenta proveedor, licencia, versión soportada, pasos de instalación y recuperación. Si no hay una salida virtual disponible, muestra claramente la limitación y permite previsualizar el audio localmente.
- Deja al usuario escoger el dispositivo de salida y formato cuando la API lo permita. Detecta dispositivo ausente, cambio de dispositivo y fallos de inicialización; libera recursos correctamente.
- Evita realimentación: no monitorices automáticamente la salida mezclada hacia la entrada virtual. Separa la preescucha local de la señal enviada a Discord.
- Verifica el flujo desde MPK Mini MK3 hasta la entrada seleccionada en Discord, incluidos niveles y latencia percibida. Un test unitario del mezclador no demuestra que Discord recibe audio.

## Mezclador por aplicación

- Controla volúmenes mediante las APIs de sesiones de audio de Windows, identificando procesos y sesiones activas de forma robusta. Una aplicación puede tener varias sesiones, cambiar PID o reiniciarse.
- Presenta aplicaciones detectadas, estado, nivel actual y control de silencio/volumen. No confundas volumen maestro del dispositivo con volumen de una sesión individual.
- Almacena preferencias según una identidad estable y validada de aplicación cuando sea posible; gestiona procesos con varias ventanas o sesiones sin asignaciones ambiguas.
- Respeta la intención del usuario: no cambies volumen global, dispositivo predeterminado ni configuración de Discord sin una acción visible y reversible.
- Gestiona cambios externos de volumen, sesiones que aparecen/desaparecen y permisos o servicios no disponibles. Evita que una sincronización genere bucles de eventos o saltos perceptibles.
- Si una API requiere COM, inicialización por hilo o una dependencia específica de Windows, encapsúlala en el adaptador y documenta ese requisito.

## Macros contextuales

- Modela una macro como acciones tipadas y permitidas, con parámetros validados, secuencia definida, límites de tiempo y cancelación. No implementes macros como ejecución arbitraria de shell o JavaScript.
- Restringe las acciones a capacidades explícitas, por ejemplo control de reproducción, cambio de volumen, selección de perfil o acciones integradas aprobadas. Cualquier acción con efectos externos debe ser visible, configurable y reversible cuando sea razonable.
- Define el contexto con señales observables y permisos claros: aplicación en primer plano, dispositivo conectado o perfil activo. Explica por qué una macro se activó y deja pausar o desactivar reglas.
- Evita activaciones dobles, recursión y conflictos de prioridad. Define precedencia cuando varias reglas coinciden, además de comportamiento ante estados desconocidos.
- Nunca captures ni almacenes texto escrito, contraseñas o contenido privado para inferir contexto. Minimiza telemetría y datos persistidos.

## Perfiles automáticos

- Permite crear, duplicar, renombrar, importar/exportar y activar perfiles. Valida esquemas, referencias MIDI, archivos de sonido y acciones antes de importar o activar.
- Los cambios automáticos de perfil deben usar reglas explícitas, prioridad determinista, histéresis o demora configurable si evita oscilaciones, y mostrar la causa del perfil activo.
- Conserva una forma sencilla de fijar manualmente un perfil o desactivar temporalmente la automatización. La desconexión de un dispositivo o la falta de contexto no debe dejar el sistema en un estado confuso.
- Versiona el formato de perfiles y migra datos antiguos de manera segura. Rechaza entradas malformadas con errores accionables; no sobrescribas perfiles válidos al importar uno inválido.

## Seguridad, privacidad y robustez

- Usa la lista de capacidades y permisos mínimos de Tauri 2. Revisa cambios en capabilities antes de ampliarlos; no habilites acceso amplio al filesystem, shell, procesos o red sin necesidad demostrada.
- Valida toda entrada que cruza de TypeScript a Rust, incluidos nombres de archivos, rutas, índices MIDI, niveles, duraciones y parámetros de macro. Evita traversal de rutas e inyección de argumentos.
- Mantén las bibliotecas y APIs actuales compatibles con Windows soportado. No incluyas secretos, tokens, rutas personales ni datos de usuario en logs o repositorio.
- Usa rutas de datos apropiadas para configuración y caché. Limita tamaños, gestiona corrupción y explica cómo recuperar o restablecer datos.
- Cierra puertos MIDI, streams de audio, event sinks, watchers y tareas al salir. Asegura comportamiento correcto ante suspensión, reanudación y cambios de dispositivo cuando aplique.

## Calidad y pruebas reales

Antes de añadir pruebas, inspecciona las existentes y usa las convenciones del repositorio. Añade pruebas relevantes para cambios de comportamiento; no sustituyas las verificaciones de hardware por mocks ni afirmes que un mock valida un dispositivo real.

- **Pruebas unitarias Rust**: validación, migraciones, reglas de perfiles, resolución de macros, parsers y lógica de mezcla que pueda aislarse.
- **Pruebas de interfaz**: tipos, estados vacíos/cargando/error, accesibilidad y renderizado de flujos críticos según el stack existente.
- **Integración**: comandos Tauri, serialización, almacenamiento y adaptadores con dobles de prueba donde resulte adecuado.
- **Verificación Windows**: compila en una versión soportada de Windows y comprueba permisos, rutas, empaquetado y arranque. Registra versión de SO y herramientas.
- **MIDI real**: conecta el MPK Mini MK3, comprueba entrada y salida de eventos, mapeo de botones/pads/teclas/controles usados, reconexión y ausencia de duplicados.
- **Audio real**: prueba salida local, dispositivo ausente y cambio de dispositivo. Si se usa controlador virtual, prueba instalación limpia, actualización, Discord seleccionado como entrada, reproducción de cada sonido y niveles durante una llamada de prueba.
- **Mezclador real**: ajusta y restaura el volumen de sesiones de más de una aplicación, incluyendo una sesión que aparece después del inicio y otra que se cierra.
- **Macros y perfiles**: verifica prioridad, cambio de contexto, pausa manual, datos inválidos y vuelta a estado conocido tras desconexión.

No declares una tarea terminada si solo compila cuando el criterio incluye el flujo real. Anota qué comandos se ejecutaron, qué equipo se usó, resultado y limitaciones. Si no hay MPK Mini MK3, dispositivo virtual o acceso a Discord disponible, deja explícito el paso pendiente.

## Packaging y distribución

- Define y documenta el proceso reproducible de desarrollo y release para Windows, con versiones fijadas o controladas de Rust, Node, gestor de paquetes, Tauri CLI y herramientas de compilación.
- Genera instaladores mediante los bundlers soportados por Tauri 2 que correspondan a los formatos objetivo. Prueba instalación, actualización, desinstalación y conservación de perfiles/sonidos en una máquina limpia o entorno equivalente.
- Inspecciona los artefactos: arquitectura, firma, nombre/editor, iconos, versión, recursos incluidos, dependencias de runtime y permisos solicitados. No distribuyas binarios o controladores externos sin licencia y procedencia verificadas.
- Firma ejecutables e instaladores cuando haya certificado y proceso de release disponible. No simules firmas ni publiques credenciales. Documenta cuándo un artefacto es de desarrollo y no está firmado.
- Mantén notas de versión, instrucciones de instalación, requisitos del sistema, solución de problemas y política para controlador virtual. Asegura que el usuario pueda restaurar sus datos antes de una actualización importante.

## Flujo de trabajo del repositorio

1. Inspecciona árbol, instrucciones adicionales, Git, manifiestos, scripts y documentación pertinente.
2. Resume internamente el flujo actual y localiza la capa responsable antes de editar.
3. Propón el cambio más pequeño que cumpla el objetivo y conserva las convenciones existentes.
4. Implementa errores, estados de carga y recuperación además del camino feliz.
5. Ejecuta verificaciones apropiadas al cambio. No instales dependencias, ejecutes builds costosos ni alteres archivos generados sin comprobar primero los scripts y requisitos del proyecto.
6. Revisa el diff completo para detectar cambios accidentales, permisos ampliados, secretos, avisos de licencia perdidos y documentación desactualizada.
7. Informa con precisión qué cambió, verificaciones realizadas y pasos reales que faltan.

## Criterios de aceptación del producto

Una entrega se considera aceptable cuando:

- La app se instala y abre en Windows soportado, conserva la configuración y ofrece diagnóstico útil tras reinicios o fallos.
- El MK3 se puede seleccionar y mapear con aprendizaje MIDI; las asignaciones siguen funcionando tras reconexión.
- El soundboard reproduce sonidos con control fiable de volumen y los envía a la entrada virtual configurada para Discord, con prevención de realimentación y recuperación ante desconexión.
- El mezclador cambia sesiones por aplicación sin afectar accidentalmente al volumen global y refleja cambios externos.
- Las macros solo ejecutan acciones permitidas, son predecibles y se pueden pausar o cancelar.
- Los perfiles automáticos cambian según reglas comprensibles, con prioridad determinista y control manual.
- La distribución informa sus requisitos, instala y desinstala limpiamente y no oculta dependencias de controladores.
- Las afirmaciones de calidad están respaldadas por resultados de pruebas automatizadas y pruebas reales apropiadas al hardware y al flujo de Discord.
