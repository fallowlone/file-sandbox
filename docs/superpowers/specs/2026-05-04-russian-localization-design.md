# Russian localization for the menu-bar app

**Date:** 2026-05-04
**Author:** brainstorm session, file-sandbox
**Status:** design — awaiting implementation plan

## Goal

Add Russian-language support to the FileSandbox macOS menu-bar app. Users pick their UI language in Settings (Auto / English / Русский); the change applies instantly without restart. Brand names, file paths, technical identifiers, and units stay English. Daemon-emitted verdict / session-state / mode strings are translated in the UI layer through a typed helper.

## Non-goals

- Localising the daemon (HTTP/JSON payloads, log lines, error messages from `src/`). Daemon stays English.
- Plurals / pluralisation rules. No `%lld files` style strings exist; if added later, extend with `variations.plural` in the catalog.
- Other languages (de/fr/es/...). Only `en` (source) and `ru`. Any unsupported system language with `localeRaw=auto` falls back to `en`.
- Translating brand names (`VirusTotal`, `pompelmi`, `Tart`, `clamd`), units (`MiB`, `min`, `days`, `Bearer`), file paths, or VM tags (`fsbx-XXXXXXXX`).
- Right-to-left layout. macOS handles RTL automatically when needed; we don't ship RTL languages here.

## Approach

**Hybrid catalog strategy** — natural English text serves as the lookup key for static UI strings (`Text("Watcher")` resolves through `Localizable.xcstrings`); daemon-emitted strings (verdict / session-state / mode) route through a typed helper `enum L` that emits explicit namespace keys (`verdict.<raw>`, `session.<raw>`, `mode.<raw>`).

This balances boilerplate (no manual constant for every UI label) with safety where it matters (renaming a daemon-driven string can't silently break translation lookups).

## Architecture

### Resources

- New file: `macos-menubar/Sources/App/Resources/Localizable.xcstrings` — single JSON catalog (Xcode 15+ format). Contains all `en` source strings plus `ru` translations.
- `Package.swift` gains `defaultLocalization: "en"` (top-level `Package(...)` parameter) and `resources: [.process("Resources")]` on the `executableTarget`.

### Locale switching

New file: `macos-menubar/Sources/App/Localization.swift`.

```swift
import SwiftUI

enum AppLocale: String, CaseIterable, Identifiable {
    case auto = "auto"
    case en   = "en"
    case ru   = "ru"
    var id: String { rawValue }

    var displayName: LocalizedStringKey {
        switch self {
        case .auto: return "Auto"
        case .en:   return "English"
        case .ru:   return "Русский"
        }
    }
}

/// Returns nil for `.auto` so SwiftUI falls back to system locale.
/// Otherwise returns an explicit Locale so the UI ignores system preferences.
func resolvedLocale(for app: AppLocale) -> Locale? {
    switch app {
    case .auto: return nil
    case .en:   return Locale(identifier: "en")
    case .ru:   return Locale(identifier: "ru")
    }
}

/// Type-safe helpers for daemon-emitted enum strings.
enum L {
    static func verdict(_ raw: String) -> LocalizedStringKey {
        LocalizedStringKey("verdict.\(raw.lowercased())")
    }
    /// Bigger pill variant in the expanded job row.
    static func verdictBig(_ raw: String) -> LocalizedStringKey {
        LocalizedStringKey("verdict.big.\(raw.lowercased())")
    }
    static func session(_ raw: String) -> LocalizedStringKey {
        LocalizedStringKey("session.\(raw.lowercased())")
    }
    static func mode(_ m: WatcherMode) -> LocalizedStringKey {
        LocalizedStringKey("mode.\(m.rawValue)")
    }
}
```

### Injection point

`App.swift` adds an `@AppStorage("filesandbox.locale") private var localeRaw: String = AppLocale.auto.rawValue`. The `MenuBarExtra` content is wrapped in `.environment(\.locale, resolvedLocale(for: AppLocale(rawValue: localeRaw) ?? .auto) ?? Locale.current)`. Any `@AppStorage` change re-evaluates the environment → SwiftUI re-renders the tree with the new locale. No restart required.

### Notifications

The launch-mode notification fires from `App.swift`'s `onChange` handler. The notification text comes from `mode.displayName` plus a fixed body string. We use `String(localized:bundle:locale:)` with the resolved locale so the notification language matches the UI selection rather than the system locale.

```swift
let l = resolvedLocale(for: AppLocale(rawValue: localeRaw) ?? .auto) ?? Locale.current
let title = String(localized: "FileSandbox started in \(modeDisplay)", locale: l)
```

## What we localise

### Static UI labels (natural keys)

Every English source string under 80 characters that appears in `Header.swift`, `Footer.swift`, `Components/StatusChip.swift`, `Components/Tabs.swift`, `Tabs/JobsTabView.swift`, `Tabs/SandboxTabView.swift`, `Tabs/SettingsTabView.swift` — except brand names, paths, VM tags, units. Full list and translations are in § Translation table below.

### Daemon-emitted strings (typed helpers)

| Source | Helper | Catalog keys |
|---|---|---|
| `SandboxJob.vt_verdict` (`infected`/`malicious`/`clean`/`inconclusive`/`oversized`) + status (`scanning`/`received`/`in_quarantine`) | `L.verdict(raw)` | `verdict.infected`, `verdict.malicious`, `verdict.clean`, `verdict.inconclusive`, `verdict.oversized`, `verdict.scanning`, `verdict.queued` |
| Same set, big-pill variant in expanded row | `L.verdictBig(raw)` | `verdict.big.infected`, `verdict.big.clean`, `verdict.big.inconclusive`, `verdict.big.oversized` |
| `SandboxSession.status` (`running`/`starting`/`stopped`/`failed`/`discarded`) | `L.session(raw)` | `session.running`, `session.starting`, `session.stopped`, `session.failed`, `session.discarded` |
| `WatcherMode` enum cases | `L.mode(self)` | `mode.active`, `mode.scan_paused`, `mode.monitoring_disabled` |

`forJobVerdict` and `bigVerdictPill()` route their `text:` parameter through these helpers instead of hard-coding English. `SessionStatePill` does the same. `StatusChip`'s label text uses `L.mode(mode)`.

### What stays English

- Brand names: `FileSandbox`, `VirusTotal`, `pompelmi`, `Tart`, `clamd`.
- File paths (`/Users/...`, `~/Library/Logs/FileSandbox/daemon.log`).
- VM tags (`fsbx-XXXXXXXX`) — rendered via `Text(verbatim: session.vmName)`.
- Units in parentheses: `(min)`, `(days)`, `(MiB)`. The label around the unit is translated; the unit itself is left in English.
- `Bearer` and `API token` strings.
- `clamd socket`, `Base VM name`, `On scan error` switch values like `Bypass to VT` (technical jargon).

## Translation table

### Header / Overflow menu

| en | ru |
|---|---|
| Disconnected | Не подключено |
| Refresh | Обновить |
| Restart daemon | Перезапустить демон |
| View logs | Открыть логи |
| Clear settled jobs | Очистить завершённые |
| Quit FileSandbox | Выйти из FileSandbox |
| Watcher mode | Режим наблюдения |

### Footer

| en | ru |
|---|---|
| Quit | Выход |

### Tabs

| en | ru |
|---|---|
| Jobs | Задачи |
| Sandbox | Песочница |
| Settings | Настройки |

### Mode (`mode.*`)

| key | en | ru |
|---|---|---|
| `mode.active` | Active | Активно |
| `mode.scan_paused` | Scanning paused | Сканирование на паузе |
| `mode.monitoring_disabled` | Monitoring disabled | Мониторинг выключен |

Mode picker labels (segmented control in Settings → Watcher → Mode):

| en | ru |
|---|---|
| Active | Активно |
| Paused | Пауза |
| Off | Выкл |

### Jobs tab

| en | ru |
|---|---|
| Watching %@ | Слежу за %@ |
| Drop files here or run a test scan | Бросьте файл сюда или запустите тестовое сканирование |
| Daemon offline | Демон не запущен |
| Start daemon | Запустить демон |
| Scanning | Сканируется |
| Quarantine | Карантин |
| Restored | Восстановлено |
| No active scans | Нет активных проверок |
| Nothing quarantined | Карантин пуст |
| Open in sandbox | Открыть в песочнице |
| Restore | Восстановить |
| Delete | Удалить |

### Verdict (`verdict.*` mini)

| key | en | ru |
|---|---|---|
| `verdict.scanning` | scanning | сканируется |
| `verdict.queued` | queued | в очереди |
| `verdict.infected` | infected | заражён |
| `verdict.malicious` | infected | заражён |
| `verdict.inconclusive` | inconclusive | не определено |
| `verdict.oversized` | oversized | слишком большой |
| `verdict.clean` | clean | чисто |

### Verdict big (`verdict.big.*`)

| key | en | ru |
|---|---|---|
| `verdict.big.infected` | Infected | Заражён |
| `verdict.big.malicious` | Infected | Заражён |
| `verdict.big.inconclusive` | Inconclusive | Не определено |
| `verdict.big.oversized` | Oversized | Слишком большой |
| `verdict.big.clean` | Clean | Чисто |

### Sandbox tab

| en | ru |
|---|---|
| New session | Новая сессия |
| Network | Сеть |
| No sandbox sessions | Нет сессий песочницы |
| Click + New session to spawn a VM | Нажмите «+ Новая сессия» чтобы запустить VM |
| Sandbox is disabled in Settings | Песочница выключена в настройках |
| Install Tart to enable | Установите Tart чтобы включить |
| Pick a file and open it in a fresh sandbox VM | Выберите файл и откройте его в свежей песочнице |
| Show window | Показать окно |
| Export | Выгрузить |
| Discard | Удалить |
| network on | сеть вкл |
| network off | сеть выкл |

### Session state (`session.*`)

| key | en | ru |
|---|---|---|
| `session.running` | running | работает |
| `session.starting` | starting | запускается |
| `session.stopped` | stopped | остановлено |
| `session.failed` | failed | сбой |
| `session.discarded` | discarded | удалено |

### Settings group headers

| en | ru |
|---|---|
| Watcher | Наблюдатель |
| Scanners | Сканеры |
| Sandbox | Песочница |
| Advanced | Дополнительно |

### Settings rows

| en | ru |
|---|---|
| Mode | Режим |
| Watch path | Папка наблюдения |
| Quarantine path | Папка карантина |
| Local (pompelmi) | Локально (pompelmi) |
| On scan error | При ошибке сканирования |
| Bypass to VT | Пропустить в VT |
| Mark inconclusive | Пометить «не определено» |
| Enable | Включить |
| Base VM name | Имя базовой VM |
| Idle timeout (min) | Таймаут простоя (min) |
| Network ON by default | Сеть ВКЛ по умолчанию |
| Output retention (days) | Хранить вывод (days) |
| Max scan size (MiB) | Макс. размер скана (MiB) |
| Max concurrent VT scans | Параллельных VT-проверок |
| Use separate VT process | Отдельный VT-процесс |
| Inconclusive retention (days) | Хранить «не определено» (days) |
| Language | Язык |

### Language picker values

| en | ru |
|---|---|
| Auto | Авто |
| English | English |
| Русский | Русский |

### Settings warning

| en | ru |
|---|---|
| No active scanners - every new file will be quarantined as inconclusive. | Нет активных сканеров — новые файлы попадут в карантин как «не определено». |

### Notifications

| en | ru |
|---|---|
| FileSandbox started in %@ | FileSandbox запущен в режиме %@ |
| New files are quarantined but not scanned. Open the menu bar to resume. | Новые файлы попадают в карантин, но не сканируются. Откройте меню для возобновления. |
| New files are not being monitored. Open the menu bar to resume. | За новыми файлами не следим. Откройте меню для возобновления. |

## File structure

| File | Status | Responsibility |
|---|---|---|
| `macos-menubar/Package.swift` | Modify | Add `defaultLocalization: "en"` + `resources: [.process("Resources")]`. |
| `macos-menubar/Sources/App/Resources/Localizable.xcstrings` | Create | All en/ru pairs (~70 keys). |
| `macos-menubar/Sources/App/Localization.swift` | Create | `AppLocale`, `resolvedLocale(for:)`, `enum L` helpers. |
| `macos-menubar/Sources/App/App.swift` | Modify | `@AppStorage("filesandbox.locale")`, `.environment(\.locale, ...)`, localised launch notification text via `String(localized:locale:)`. |
| `macos-menubar/Sources/App/JobStore.swift` | Modify | `WatcherMode.displayName` returns `LocalizedStringKey` (or stays `String` and routes via `L.mode(self)` at call sites — pick one). |
| `macos-menubar/Sources/App/Components/StatusChip.swift` | Modify | Use `L.mode(mode)` for chip label; `Disconnected` natural key. |
| `macos-menubar/Sources/App/Components/Tabs.swift` | Modify | `tab.title` returns `LocalizedStringKey`. |
| `macos-menubar/Sources/App/Components/VerdictPill.swift` | Modify | `text:` parameter accepts `LocalizedStringKey`. `forJobVerdict` and `SessionStatePill` route through `L.verdict(...)` / `L.session(...)`. |
| `macos-menubar/Sources/App/Tabs/JobsTabView.swift` | Modify | `bigVerdictPill()` uses `L.verdictBig(raw)`. Static `Text("...")` strings stay (auto-localised). |
| `macos-menubar/Sources/App/Tabs/SandboxTabView.swift` | Modify | Static `Text("...")` strings stay. `IconButton.help` accepts `LocalizedStringKey`. State pill routes through `L.session(...)`. |
| `macos-menubar/Sources/App/Tabs/SettingsTabView.swift` | Modify | New `Language` row inside Advanced group with `Picker` bound to `localeRaw`. |

## Behaviour details

- **`WatcherMode.displayName` return type:** the spec keeps it as `String` and routes to `LocalizedStringKey` at call sites via `L.mode(self)`. Reason: it's also used in `String(localized:)` for the notification text, which expects a string-compatible source. Returning `LocalizedStringKey` would force callers to convert. The implementation plan picks the exact site changes.
- **`@AppStorage` propagation:** SwiftUI re-evaluates the View hierarchy whenever a tracked `@AppStorage` value changes. The `.environment(\.locale, ...)` modifier is downstream, so its argument re-computes each render → tree gets re-evaluated with new locale → all `Text(LocalizedStringKey)` re-resolve.
- **Bundle resolution:** `.process("Resources")` copies the `.xcstrings` into the SwiftPM bundle. SwiftUI uses `Bundle.module` for SwiftPM targets (auto-injected). For the bundled `.app` (built via `build.sh`), SwiftPM's resource bundle is embedded inside `Contents/Resources/FileSandboxMenuBar_FileSandboxMenuBar.bundle/`. Verify this works post-build. If not, fallback: copy the `.xcstrings`-derived `.lproj` outputs directly into `Contents/Resources/` in `build.sh`.
- **Locale fallback:** for `localeRaw=auto` and a system locale we don't have (`de`/`fr`/`es`/...), the catalog falls back to `en` per `defaultLocalization: "en"`. No crash, no missing strings.

## Failure modes

| Situation | Behaviour |
|---|---|
| Catalog key missing | `Text("Watcher")` → SwiftUI returns the literal "Watcher" as fallback. No crash. |
| Daemon emits unknown verdict (e.g. `error`) | `Text(L.verdict("error"))` → `verdict.error` not in catalog → fallback shows the string `verdict.error`. Mitigation: `forJobVerdict`'s default branch already returns a grey-pill with the raw value via a separate `Text(verbatim: v)` rather than `Text(L.verdict(v))`. |
| Long Russian translation truncates row | Existing `.lineLimit(1)` + `.truncationMode(.middle/.tail)` already cover Job rows and filename labels. Settings rows use flex labels, so wrapping handles them. |
| User flips Language while sandbox session is starting | No effect on the daemon. UI rerenders, session state pill switches language at next render tick. |
| Notification fires before `localeRaw` is read | `@AppStorage` is initialised eagerly on `App.body` evaluation; the notification handler runs after first `onChange`, so `localeRaw` is always read. Edge case if user has just installed the app: defaults to `auto` → system locale. |
| `.xcstrings` build resource not embedded by `build.sh` | `Text("Watcher")` returns `"Watcher"` (English). This is a packaging bug, not a runtime crash. Verified manually after first build. |

## Migration

- New `@AppStorage` key `filesandbox.locale` (defaults to `"auto"`). No migration of existing keys.
- Existing English-only users with `localeRaw=auto` and English system locale see no change.
- Existing English-only users with `localeRaw=auto` and Russian system locale will start seeing Russian UI on first launch after this ships. Acceptable; if they want to lock English, they pick `English` in the new Settings → Advanced → Language row.

## Acceptance checklist (manual)

1. `swift build` clean. `./build.sh` produces a working `.app`.
2. First launch with default settings: UI matches system locale (English on en-system, Russian on ru-system).
3. Settings → Advanced → Language → `English` → header `Disconnected`/`Active`, tabs `Jobs`/`Sandbox`/`Settings`, footer `Restart daemon · View logs` `Quit`.
4. Settings → Advanced → Language → `Русский` → header `Не подключено`/`Активно`, tabs `Задачи`/`Песочница`/`Настройки`, footer `Перезапустить демон · Открыть логи` `Выход`.
5. Switching language is instant. No app restart, no menu close-and-reopen needed.
6. Verdict pills with `vt_verdict=infected` show `заражён` (mini) / `Заражён` (big) on Russian.
7. Sandbox row state pills show `работает` / `запускается` on Russian.
8. Mode chip click → menu items `Активно` / `Сканирование на паузе` / `Мониторинг выключен` on Russian.
9. Restart app with `localeRaw=ru` → language is preserved.
10. Launch with watcherMode=`scan_paused` → notification body is Russian when `localeRaw=ru`.
11. Brands and units stay English: `VirusTotal`, `pompelmi`, `Tart`, `clamd`, `MiB`, `min`, `days`, `Bearer`, `fsbx-XXXXXXXX`, paths.
12. Unknown verdict (`error`) renders raw `error` text without crashing.

## Open work

- The plan should pick exactly how `WatcherMode.displayName` flips: stay `String` and let call sites use `L.mode(self)` (recommended; minimal API change), OR change the return type and update the notification site to use `String(localized:)` on the LocalizedStringKey. The implementation plan picks one and applies it consistently.
- Confirm SwiftPM `.process("Resources")` produces a bundle that the runtime resolves under macOS 14+ when launched from the `.app` shell. If not, the plan adds a `build.sh` step that copies the synthesised `.lproj` into `Contents/Resources/`.
- Future work (out of scope): plurals via `variations.plural` if any list-count copy is added (e.g. `"%lld files in quarantine"`).
