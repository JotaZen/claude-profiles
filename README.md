# Claude Profiles

Aplicación de escritorio (Tauri 2 + Rust) para tener varias cuentas de Claude en el mismo PC:
una personal y una del trabajo, sin cerrar sesión cada vez.

## Qué hace, y por qué funciona así

Claude guarda la sesión en dos sitios distintos, y cada uno admite un truco diferente.

### Claude Code (CLI)

Toda la identidad vive bajo la variable `CLAUDE_CONFIG_DIR`. Verificado en este equipo:
al fijarla, Claude Code crea dentro de ese directorio **tanto** `.credentials.json` **como**
`.claude.json`, así que un perfil es autocontenido.

Consecuencia: **varias cuentas pueden correr en paralelo**, cada una en su terminal. No hay
bloqueo ni carpeta compartida.

La app crea un perfil por cuenta en `<raíz>\<perfil>\code`, genera un `claude-code.cmd` que
exporta la variable y lanza `claude`, y opcionalmente fija ese perfil como el de por defecto
del usuario (`setx CLAUDE_CONFIG_DIR`) para cuando escribas `claude` a mano.

### Claude Desktop

Es una app Electron empaquetada como MSIX. Toda la sesión (cookies, tokens cifrados con
DPAPI, configuración de MCP, historial) vive en `%APPDATA%\Claude`. Hay dos caminos:

1. **Cambio de perfil (recomendado).** La app mueve `%APPDATA%\Claude` a
   `<raíz>\<perfil>\desktop` y deja una *junction* de NTFS en su lugar. Cambiar de cuenta es
   reapuntar la junction: instantáneo, sin copiar nada y sin permisos de administrador.
   Requiere que Claude Desktop esté cerrado.
2. **Instancias en paralelo.** Electron acepta `--user-data-dir`, y Chromium ancla su bloqueo
   de instancia única a ese directorio, así que dos directorios distintos son dos apps
   distintas. La app intenta lanzar el ejecutable del paquete directamente; si Windows lo
   bloquea (los binarios en `WindowsApps` suelen estar restringidos), se usa una copia
   portable del paquete (~660 MB) que la propia app crea con `robocopy /MIR`.

   La copia portable no se actualiza sola: tras cada actualización de Claude hay que pulsar
   "Actualizar copia".

## Primer uso

Con Claude Desktop cerrado, pulsa **Configurar automáticamente**. La app:

1. crea el perfil `Personal` y copia en él la sesión de Claude Code que ya tienes,
2. mueve `%APPDATA%\Claude` a ese perfil y deja la junction,
3. lo marca como perfil por defecto de la CLI,
4. crea un perfil `Trabajo` vacío, con tu configuración personal (skills, `CLAUDE.md`,
   `settings.json`...) pero sin credenciales.

Después, para la segunda cuenta: pulsa **Usar** en `Trabajo` y abre Claude Desktop (pedirá
iniciar sesión) o pulsa **Terminal** y ejecuta `/login`.

A partir de ahí hay un solo botón por perfil:

- **Usar** — deja ese perfil activo en el escritorio y en la CLI. Si Claude Desktop está
  abierto, la app ofrece cerrarlo.
- **Terminal** — abre una terminal con ese perfil **sin** cambiar el activo. Puedes tener
  varias a la vez, una por cuenta.
- **⋯** — segunda ventana de Claude en paralelo, copiar configuración, acceso directo en el
  escritorio, editar, eliminar.

La sección *Avanzado* muestra el estado real (dónde apunta cada cosa) y permite deshacerlo
todo con *Dejar de gestionar*: devuelve la carpeta del perfil activo a `%APPDATA%\Claude` y
borra la junction.

## Qué lee la app

Nada sale del equipo. Para mostrar quién es cada perfil lee:

- `<perfil>\code\.claude.json` → `oauthAccount.emailAddress`, `organizationName`, rol.
- `<perfil>\code\.credentials.json` → sólo `claudeAiOauth.subscriptionType` (el plan). Los
  tokens no se leen ni se muestran.
- `<perfil>\desktop\config.json` → `lastKnownAccountUuid`. El token del escritorio está
  cifrado con DPAPI, así que el correo de esa cuenta no es legible: por eso cada perfil lleva
  la etiqueta que tú le pongas.

Su propia configuración vive en `%APPDATA%\ClaudeProfiles\config.json`.

## Compilar

Necesita Rust (toolchain MSVC), VS Build Tools y WebView2 (ya viene en Windows 11).

```bat
build.cmd
```

El script fija `CARGO_TARGET_DIR` fuera del árbol del proyecto. Es a propósito: compilando
dentro de `%APPDATA%` el enlazador falla con `LNK1104` porque Defender mantiene abiertos los
ejecutables recién creados. Si mueves el proyecto a una carpeta normal, `cargo build
--release` basta.

El binario queda en `%CARGO_TARGET_DIR%\release\claude-profiles.exe`.

## Límites conocidos

- Sólo Windows: junctions, `setx`, `robocopy` y rutas MSIX.
- Cambiar el perfil del escritorio exige cerrar Claude Desktop (la app lo detecta y ofrece
  cerrarlo).
- Las instancias en paralelo del escritorio comparten el mismo registro de la app en Windows;
  si una actualización automática reescribe el paquete, la copia portable queda desfasada
  hasta que la regeneres.
- El correo de la cuenta del escritorio no se puede mostrar (DPAPI).
