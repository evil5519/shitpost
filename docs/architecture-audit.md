# Auditoría arquitectónica

**Alcance:** auditoría estructural del codebase actual. No se modificaron archivos existentes; este reporte es el único archivo creado en esta fase. No se proponen features ni cambios de negocio.

## 1. Mapa actual de crates y módulos

| Unidad | Archivo | Responsabilidad |
|---|---|---|
| Paquete/binario `shitpost` | `src/main.rs` | Bootstrap nativo/WASM, logging, viewport, canvas web y arranque de `ui::PortfolioApp`. |
| Paquete/lib crate `shitpost` | `src/lib.rs` | Crate raíz vacío de lógica; permanece como target de librería del paquete. |
| Crate `core` | `crates/core/src/lib.rs` | Estado de dominio, snapshots, comandos, navegación y dispatch de las cuatro features de negocio. |
| `core::portfolio` | `crates/core/src/portfolio.rs` | Portfolio, proyectos, validación y transiciones de portfolio. |
| `core::calculator` | `crates/core/src/calculator.rs` | REPL, historial, runtime no serializado, restore/migración legacy y comandos de calculadora. |
| `core::text_analyzer` | `crates/core/src/text_analyzer.rs` | Texto persistido, estadísticas deterministas y comando de actualización. |
| `core::color_converter` | `crates/core/src/color_converter.rs` | RGB/hex, validación, canonicalización, errores estructurados y comandos de color. |
| Crate `ui` | `crates/ui/src/lib.rs`, `crates/ui/src/app.rs` | Adaptador egui/eframe, render, eventos UI, focus, ventanas y almacenamiento eframe. |
| Crate `calculator-engine` | `crates/calculator-engine/src/lib.rs` | Parser y motor numérico sin dependencias egui/eframe. |

`src/app.rs` fue eliminado después de confirmar que no tenía imports desde el workspace; `src/lib.rs` ya no declara `mod app` ni reexporta `PortfolioApp`.

## 2. Lógica dentro de callbacks egui

### Criterio

Se marca una ocurrencia cuando una closure de render ejecuta mutación de estado persistente, navegación, validación, cálculo o un side-effect. No se encontraron I/O ni llamadas async dentro de callbacks egui. Sí se encontró mutación y lógica de negocio local.

| Archivo:línea | Ocurrencia | Tipo |
|---|---|---|
| `src/app.rs:513-514` | `Quit` ejecuta `ui.send_viewport_cmd(ViewportCommand::Close)` | Side-effect de plataforma/UI |
| `src/app.rs:527-529` | Menú Portfolio llama `self.workspace.activate_view(view, false)` | Transición de navegación |
| `src/app.rs:540-542` | Menú Tools llama `self.workspace.activate_view(view, false)` | Transición de navegación |
| `src/app.rs:651-652` | Botón Home muta `self.workspace.mobile_view` | Transición de navegación |
| `src/app.rs:662-665` | Menú móvil Portfolio llama `activate_view` | Transición de navegación |
| `src/app.rs:673-675` | Menú móvil Tools llama `activate_view` | Transición de navegación |
| `src/app.rs:567-569` | Resultado de `show_home` ejecuta `activate_view` | Evento y transición mezclados |
| `src/app.rs:726-727` | Resultado móvil de `show_home` ejecuta `activate_view` | Evento y transición mezclados |
| `src/app.rs:979-980` | `Add project` hace `content.projects.push(...)` | Mutación persistente de dominio |
| `src/app.rs:1043-1050` | Tab reemplaza input y llama `refresh_preview` | Edición + cálculo |
| `src/app.rs:1052-1069` | Flechas navegan historial y recalculan preview | Edición + cálculo |
| `src/app.rs:1077-1079` | `response.changed()` llama `refresh_preview` | Validación/cálculo en render |
| `src/app.rs:1081-1083` | Enter ejecuta `state.evaluate()` y pide focus | Side-effect + negocio |
| `src/app.rs:1090-1097` | Completion modifica input, recalcula y pide focus | Edición + cálculo |
| `src/app.rs:1143-1151` | `Apply hex` valida, muta RGB/input y error | Validación + mutación persistente |
| `src/app.rs:1158-1168` | Sliders normalizan RGB a hex y limpian error | Lógica de dominio en render |

También hay acoplamiento de persistencia fuera de callbacks: `src/app.rs:471-477` usa `eframe::get_value` y restaura runtime; `src/app.rs:495-497` usa `eframe::set_value`. El modelo persistente queda así atado al framework.

La gravedad actual es moderada: no hay red, disco ni async en esos callbacks. El problema es la ausencia de una frontera compilable entre evento, comando, transición de dominio, persistencia y render.

## 3. Separación física propuesta

### Dependencias objetivo

```text
shitpost binary -> ui -> core
core -> calculator-engine   (si el slice calculator lo necesita)
core -/-> ui
core -/-> egui
core -/-> eframe
```

Estructura propuesta:

```text
crates/
  core/src/lib.rs
  core/src/portfolio.rs
  core/src/calculator.rs
  core/src/text_analyzer.rs
  core/src/color_converter.rs
  core/src/session.rs
  ui/src/lib.rs
  ui/src/app.rs
  ui/src/navigation.rs
  ui/src/views.rs
src/main.rs
```

### `core`

Debe compilar sin egui, eframe, APIs de ventana ni backend gráfico. Debe contener:

- estado de dominio y sesión persistible;
- invariantes y validación de portfolio, enlaces, email y color;
- transiciones mediante comandos/mensajes;
- cálculo derivado de texto y previews;
- integración con `calculator-engine`;
- snapshots, versionado y migraciones;
- codec o estructuras serializables independientes de `eframe::Storage`.

La persistencia debe dividirse así: `core` define snapshot, versión, migraciones y serialización; `ui`/bootstrap adapta `eframe::Storage` o `localStorage` a ese snapshot. De este modo `core` conserva la persistencia del dominio sin importar eframe.

API conceptual:

```rust
pub enum Command {
    Navigate(View),
    AddProject,
    EditPortfolio(PortfolioField, String),
    Calculator(CalculatorCommand),
    TextAnalyzer(TextCommand),
    ColorConverter(ColorCommand),
}

pub struct CoreState { /* datos y runtime de dominio */ }

impl CoreState {
    pub fn dispatch(&mut self, command: Command) -> CoreResult<Effect>;
    pub fn snapshot(&self) -> PersistedState;
}
```

Los nombres son ilustrativos; no justifican una abstracción genérica adicional.

### `ui`

Debe depender de `core`, egui y eframe, y contener solamente:

- `PortfolioApp` como adaptador de eframe;
- widgets, ventanas, layout responsive y estilos;
- traducción de clicks, texto, teclas y sliders a comandos;
- presentación de resultados y diagnósticos;
- focus, ids, scroll y geometría egui;
- adaptación del almacenamiento eframe.

El flujo debe ser:

```text
render widgets -> collect Command(s) -> dispatch en core -> render nuevo estado
```

La UI no debe mutar directamente campos persistentes. `ui.close()` y `ViewportCommand::Close` son detalles de UI y no deben existir en `core`.

`src/main.rs` debe quedar como bootstrap: selección native/WASM, logging, viewport/canvas y construcción de `ui::PortfolioApp`.

## 4. Vertical slices dentro de `core`

La división debe ser por feature/caso de uso, no por `models/services/repositories`.

### `core::portfolio`

`PortfolioContent`, `Project`, edición de campos, agregar proyectos, validación de email/URLs y proyecciones About/Projects/Contact.

### `core::calculator`

Input, historial, sesión, comandos de completar/navegar/evaluar, integración con `calculator-engine`, restore report y migración del formato legado.

### `core::text_analyzer`

Texto persistido, comando de reemplazo y estadísticas deterministas de caracteres, palabras y líneas.

### `core::color_converter`

RGB, hex, parseo, canonicalización, validación, `ApplyHex`, `SetChannel` y errores estructurados sin estado visual egui.

### `core::session`

Snapshot persistible compuesto, versionado, defaults y migraciones. No debe contener geometry, focus, IDs egui ni flags de ventanas.

`WorkspaceState` debe dividirse: navegación semántica puede pertenecer a `core` si es parte del caso de uso; posiciones, tamaños, ventanas abiertas, focus y errores puramente visuales deben quedar en `ui`.

## 5. Guardrails deterministas

### Situación actual

Existe un comando único para el gate principal:

```sh
env -u NO_COLOR ./check.sh
```

`check.sh` ejecuta workspace check, check WASM, rustfmt, Clippy con `-D warnings`, tests de workspace, doctests y `trunk build`.

No hay configuración observada de `cargo deny`, `cargo audit` ni política equivalente de licencias/vulnerabilidades. Tampoco hay hook versionado de git, pre-commit o hook de OMP que ejecute el gate. `.mcp.json` solo configura el servidor egui MCP. Las reglas de `AGENTS.md` son instrucciones para agentes, no enforcement del compilador.

### Limitaciones

- `NO_COLOR` debe desactivarse para Trunk 0.21.14; está documentado, pero el script no lo encapsula.
- `trunk` no está fijado por versión en el repositorio.
- CI y `check.sh` no son literalmente el mismo pipeline: CI separa jobs y usa argumentos distintos.
- No existe todavía una frontera de Cargo que impida importar egui desde el dominio ni mutar dominio desde UI.

### Guardrails objetivo

1. `core/Cargo.toml` sin egui/eframe.
2. `ui/Cargo.toml` con dependencia explícita de `core` y egui/eframe.
3. El binario depende de `ui`, no de detalles internos de `core`.
4. Un comando común local/CI para check, fmt, clippy, tests y WASM.
5. Tests de `core` ejecutables sin backend gráfico.
6. CI compilando `core` aisladamente.
7. Si se necesita enforcement de dependencias, una política reproducible como `cargo deny` integrada en script y CI.


## Estado de cierre de la migración

- `core` y `ui` existen como crates físicos con dependencia `ui -> core`.
- `Command` es un enum central con variantes directas para navegación, portfolio, calculator, text analyzer y color converter.
- `.githooks/pre-commit` ejecuta `env -u NO_COLOR ./check.sh` y está activado mediante `core.hooksPath` en este checkout.
- Los cuatro slices de negocio, navegación y tests de dominio fueron migrados.
- `src/app.rs` raíz fue eliminado; queda pendiente revisar si el target de librería raíz vacío debe conservarse en una futura simplificación separada.
## Dictamen

La separación correcta es física, no solamente por carpetas dentro del crate existente. El primer corte debe mover estado persistente, validación, migraciones y transiciones de las features a `core`; después `ui` traducirá eventos egui a comandos. Cargo y el compilador harán cumplir que `core` no dependa de egui/eframe y que `ui` sea la única capa de renderizado. No se recomienda introducir funcionalidades de negocio durante esa refactorización.
