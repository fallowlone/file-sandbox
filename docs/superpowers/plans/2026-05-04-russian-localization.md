# Russian localization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Russian-language UI for the menu-bar app with an in-app `Auto / English / Русский` switcher, instant locale change, and a hybrid catalog strategy (natural keys for static UI, namespaced keys via `L` helpers for daemon-emitted strings).

**Architecture:** Single `Localizable.xcstrings` under `Sources/App/Resources/`. SwiftPM `defaultLocalization: "en"` + `.process("Resources")` resource. `AppLocale` enum + `resolvedLocale(for:)` + `enum L` typed helpers in a new `Localization.swift`. `App.swift` wraps the menu-bar content in `.environment(\.locale, …)` driven by `@AppStorage("filesandbox.locale")`. Settings → Advanced gets a `Language` Picker. Daemon stays untouched.

**Tech Stack:** SwiftUI (macOS 14+), SwiftPM, Xcode-15-style String Catalog (`.xcstrings`), `LocalizedStringKey`, `String(localized:locale:)`.

---

## Pre-flight

- Branch: continue on `feat/menubar-ui-redesign` (already off `main`, contains the redesign + this spec). The localization work lands here too — same feature branch — so the next merge ships UI redesign + RU localization as one feature.
- No XCTest target. Verification per task = `cd macos-menubar && swift build` returns `Build complete!`. Final manual acceptance done in Task 12.
- Branch base SHA at start of this plan: `138609e` (the spec commit).

---

## File structure

| File | Status | Responsibility |
|---|---|---|
| `macos-menubar/Package.swift` | Modify (Task 1) | Top-level `defaultLocalization: "en"`, target `resources: [.process("Resources")]`. |
| `macos-menubar/Sources/App/Resources/Localizable.xcstrings` | Create (Task 2) | Single JSON catalog, 60+ keys, en + ru pairs. |
| `macos-menubar/Sources/App/Localization.swift` | Create (Task 3) | `AppLocale`, `resolvedLocale(for:)`, `enum L` helpers. |
| `macos-menubar/Sources/App/JobStore.swift` | Modify (Task 4) | Add `WatcherMode.displayKey: LocalizedStringKey`. Keep `displayName: String` for non-localized contexts. |
| `macos-menubar/Sources/App/App.swift` | Modify (Task 5) | `@AppStorage("filesandbox.locale")`, `.environment(\.locale, ...)`, localized launch-notification text via `String(localized:locale:)`. |
| `macos-menubar/Sources/App/Components/StatusChip.swift` | Modify (Task 6) | Use `mode.displayKey` instead of `mode.displayName`; `Disconnected` stays as natural-key `Text`. |
| `macos-menubar/Sources/App/Components/Tabs.swift` | Modify (Task 7) | `AppTab.title` returns `LocalizedStringKey` (not `String`). |
| `macos-menubar/Sources/App/Components/VerdictPill.swift` | Modify (Task 8) | `text:` parameter accepts `LocalizedStringKey`. `forJobVerdict` routes via `L.verdict(...)`. `SessionStatePill` routes via `L.session(...)`. |
| `macos-menubar/Sources/App/Tabs/JobsTabView.swift` | Modify (Task 9) | `bigVerdictPill()` uses `L.verdictBig(raw)`. Static `Text("...")` strings unchanged (auto-localized). |
| `macos-menubar/Sources/App/Tabs/SandboxTabView.swift` | Modify (Task 10) | `IconButton.help` parameter typed as `LocalizedStringKey`. Other strings unchanged. |
| `macos-menubar/Sources/App/Tabs/SettingsTabView.swift` | Modify (Task 11) | New `Language` row in Advanced group, bound to `@AppStorage("filesandbox.locale")` via Picker over `AppLocale.allCases`. |

Acceptance — Task 12 (manual).

---

## Task 1: Package.swift — declare resources + default localization

**Files:**

- Modify: `macos-menubar/Package.swift`

- [ ] **Step 1: Replace the file content with this exact content**

```swift
// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "FileSandboxMenuBar",
    defaultLocalization: "en",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "FileSandboxMenuBar",
            path: "Sources/App",
            resources: [
                .process("Resources")
            ]
        ),
    ]
)
```

- [ ] **Step 2: Create the empty Resources folder so the build succeeds even before Task 2 lands**

```bash
mkdir -p /Users/artemmac/dev/personal/file-sandbox/macos-menubar/Sources/App/Resources
```

- [ ] **Step 3: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

> If SwiftPM complains "specified resources but none were found", that means `Resources/` is empty. Add a placeholder file `Sources/App/Resources/.gitkeep` and re-run. (Task 2 replaces this with the catalog.)

- [ ] **Step 4: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Package.swift macos-menubar/Sources/App/Resources
git commit -m "build(menubar): declare Resources + defaultLocalization en"
```

---

## Task 2: Localizable.xcstrings — full en/ru catalog

**Files:**

- Create: `macos-menubar/Sources/App/Resources/Localizable.xcstrings`

The catalog ships every translation table from the spec. The keys below are organised into the same groups as the spec for readability — but in the JSON they all sit flat under the single `strings` object.

- [ ] **Step 1: Write the catalog**

Write this exact content to `macos-menubar/Sources/App/Resources/Localizable.xcstrings`:

```json
{
  "sourceLanguage" : "en",
  "version" : "1.0",
  "strings" : {
    "Active" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Active" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Активно" } } } },
    "Advanced" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Advanced" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Дополнительно" } } } },
    "API token" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "API token" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "API token" } } } },
    "Auto" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Auto" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Авто" } } } },
    "Base VM name" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Base VM name" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Имя базовой VM" } } } },
    "Bypass to VT" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Bypass to VT" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Пропустить в VT" } } } },
    "Clear settled jobs" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Clear settled jobs" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Очистить завершённые" } } } },
    "Click + New session to spawn a VM" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Click + New session to spawn a VM" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Нажмите «+ Новая сессия» чтобы запустить VM" } } } },
    "clamd socket" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "clamd socket" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "clamd socket" } } } },
    "Daemon offline" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Daemon offline" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Демон не запущен" } } } },
    "Delete" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Delete" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Удалить" } } } },
    "Discard" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Discard" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Удалить" } } } },
    "Disconnected" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Disconnected" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Не подключено" } } } },
    "Drop files here or run a test scan" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Drop files here or run a test scan" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Бросьте файл сюда или запустите тестовое сканирование" } } } },
    "Enable" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Enable" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Включить" } } } },
    "English" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "English" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "English" } } } },
    "Export" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Export" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Выгрузить" } } } },
    "FileSandbox started in %@" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "FileSandbox started in %@" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "FileSandbox запущен в режиме %@" } } } },
    "Idle timeout (min)" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Idle timeout (min)" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Таймаут простоя (min)" } } } },
    "Inconclusive retention (days)" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Inconclusive retention (days)" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Хранить «не определено» (days)" } } } },
    "Install Tart to enable" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Install Tart to enable" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Установите Tart чтобы включить" } } } },
    "Jobs" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Jobs" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Задачи" } } } },
    "Language" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Language" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Язык" } } } },
    "Local (pompelmi)" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Local (pompelmi)" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Локально (pompelmi)" } } } },
    "Mark inconclusive" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Mark inconclusive" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Пометить «не определено»" } } } },
    "Max concurrent VT scans" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Max concurrent VT scans" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Параллельных VT-проверок" } } } },
    "Max scan size (MiB)" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Max scan size (MiB)" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Макс. размер скана (MiB)" } } } },
    "Mode" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Mode" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Режим" } } } },
    "Network" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Network" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Сеть" } } } },
    "network off" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "network off" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "сеть выкл" } } } },
    "network on" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "network on" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "сеть вкл" } } } },
    "Network ON by default" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Network ON by default" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Сеть ВКЛ по умолчанию" } } } },
    "New session" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "New session" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Новая сессия" } } } },
    "New files are quarantined but not scanned. Open the menu bar to resume." : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "New files are quarantined but not scanned. Open the menu bar to resume." } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Новые файлы попадают в карантин, но не сканируются. Откройте меню для возобновления." } } } },
    "New files are not being monitored. Open the menu bar to resume." : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "New files are not being monitored. Open the menu bar to resume." } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "За новыми файлами не следим. Откройте меню для возобновления." } } } },
    "No active scanners - every new file will be quarantined as inconclusive." : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "No active scanners - every new file will be quarantined as inconclusive." } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Нет активных сканеров — новые файлы попадут в карантин как «не определено»." } } } },
    "No active scans" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "No active scans" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Нет активных проверок" } } } },
    "No sandbox sessions" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "No sandbox sessions" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Нет сессий песочницы" } } } },
    "Nothing quarantined" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Nothing quarantined" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Карантин пуст" } } } },
    "Off" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Off" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Выкл" } } } },
    "On scan error" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "On scan error" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "При ошибке сканирования" } } } },
    "Open in sandbox" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Open in sandbox" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Открыть в песочнице" } } } },
    "Output retention (days)" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Output retention (days)" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Хранить вывод (days)" } } } },
    "Paused" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Paused" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Пауза" } } } },
    "Pick a file and open it in a fresh sandbox VM" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Pick a file and open it in a fresh sandbox VM" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Выберите файл и откройте его в свежей песочнице" } } } },
    "Quarantine" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Quarantine" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Карантин" } } } },
    "Quarantine path" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Quarantine path" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Папка карантина" } } } },
    "Quit" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Quit" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Выход" } } } },
    "Quit FileSandbox" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Quit FileSandbox" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Выйти из FileSandbox" } } } },
    "Refresh" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Refresh" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Обновить" } } } },
    "Restart daemon" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Restart daemon" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Перезапустить демон" } } } },
    "Restore" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Restore" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Восстановить" } } } },
    "Restored" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Restored" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Восстановлено" } } } },
    "Russian" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Русский" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Русский" } } } },
    "Sandbox" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Sandbox" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Песочница" } } } },
    "Sandbox is disabled in Settings" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Sandbox is disabled in Settings" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Песочница выключена в настройках" } } } },
    "Scanners" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Scanners" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Сканеры" } } } },
    "Scanning" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Scanning" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Сканируется" } } } },
    "Settings" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Settings" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Настройки" } } } },
    "Show window" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Show window" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Показать окно" } } } },
    "Start daemon" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Start daemon" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Запустить демон" } } } },
    "Use separate VT process" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Use separate VT process" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Отдельный VT-процесс" } } } },
    "View logs" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "View logs" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Открыть логи" } } } },
    "VirusTotal" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "VirusTotal" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "VirusTotal" } } } },
    "VT API key" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "VT API key" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "VT API key" } } } },
    "Watcher" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Watcher" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Наблюдатель" } } } },
    "Watcher mode" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Watcher mode" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Режим наблюдения" } } } },
    "Watching %@" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Watching %@" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Слежу за %@" } } } },
    "Watch path" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Watch path" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Папка наблюдения" } } } },
    "mode.active" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Active" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Активно" } } } },
    "mode.scan_paused" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Scanning paused" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Сканирование на паузе" } } } },
    "mode.monitoring_disabled" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Monitoring disabled" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Мониторинг выключен" } } } },
    "verdict.scanning" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "scanning" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "сканируется" } } } },
    "verdict.queued" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "queued" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "в очереди" } } } },
    "verdict.infected" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "infected" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "заражён" } } } },
    "verdict.malicious" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "infected" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "заражён" } } } },
    "verdict.inconclusive" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "inconclusive" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "не определено" } } } },
    "verdict.oversized" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "oversized" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "слишком большой" } } } },
    "verdict.clean" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "clean" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "чисто" } } } },
    "verdict.big.infected" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Infected" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Заражён" } } } },
    "verdict.big.malicious" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Infected" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Заражён" } } } },
    "verdict.big.inconclusive" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Inconclusive" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Не определено" } } } },
    "verdict.big.oversized" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Oversized" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Слишком большой" } } } },
    "verdict.big.clean" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "Clean" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "Чисто" } } } },
    "session.running" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "running" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "работает" } } } },
    "session.starting" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "starting" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "запускается" } } } },
    "session.stopped" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "stopped" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "остановлено" } } } },
    "session.failed" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "failed" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "сбой" } } } },
    "session.discarded" : { "localizations" : {
      "en" : { "stringUnit" : { "state" : "translated", "value" : "discarded" } },
      "ru" : { "stringUnit" : { "state" : "translated", "value" : "удалено" } } } }
  }
}
```

- [ ] **Step 2: Verify build still passes**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

> SwiftPM compiles the catalog into the resource bundle. The catalog itself doesn't trigger Swift code changes — build is only verifying SwiftPM accepts the file format.

- [ ] **Step 3: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/Resources/Localizable.xcstrings
git rm macos-menubar/Sources/App/Resources/.gitkeep 2>/dev/null || true
git commit -m "feat(menubar): Localizable.xcstrings catalog (en + ru)"
```

---

## Task 3: Localization.swift — `AppLocale`, `resolvedLocale`, `enum L`

**Files:**

- Create: `macos-menubar/Sources/App/Localization.swift`

- [ ] **Step 1: Write `Localization.swift` with this exact content**

```swift
import SwiftUI

/// User-selected UI language.
/// `auto` defers to the system locale; otherwise the chosen identifier wins.
enum AppLocale: String, CaseIterable, Identifiable {
    case auto = "auto"
    case en   = "en"
    case ru   = "ru"

    var id: String { rawValue }

    /// Label shown in the Settings → Language picker.
    var displayName: LocalizedStringKey {
        switch self {
        case .auto: return "Auto"
        case .en:   return "English"
        case .ru:   return "Russian"
        }
    }
}

/// Returns nil for `.auto` so SwiftUI falls back to the system locale.
/// Otherwise returns an explicit Locale so the UI ignores system preferences.
func resolvedLocale(for app: AppLocale) -> Locale? {
    switch app {
    case .auto: return nil
    case .en:   return Locale(identifier: "en")
    case .ru:   return Locale(identifier: "ru")
    }
}

/// Type-safe helpers that produce LocalizedStringKey values for daemon-emitted
/// enum strings. Keeps the catalog keys (verdict.*, session.*, mode.*) in one
/// place — renaming a daemon string only requires touching this file.
enum L {
    static func verdict(_ raw: String) -> LocalizedStringKey {
        LocalizedStringKey("verdict.\(raw.lowercased())")
    }
    /// Bigger pill variant in the expanded job row (sentence-cased values).
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

- [ ] **Step 2: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/Localization.swift
git commit -m "feat(menubar): Localization.swift (AppLocale + L helpers)"
```

---

## Task 4: WatcherMode — add `displayKey`

**Files:**

- Modify: `macos-menubar/Sources/App/JobStore.swift`

- [ ] **Step 1: Add `displayKey` to `WatcherMode`**

Open `macos-menubar/Sources/App/JobStore.swift`. Find the existing `var displayName: String { switch self { case .active: return "Active" ... } }` block (around line 9). Immediately AFTER the closing brace of `displayName`'s computed property and BEFORE `var symbolName: String { ... }`, insert this property:

```swift
    /// Localized display name for views (`StatusChip`, picker labels).
    /// Routes through the catalog `mode.<rawValue>` keys.
    var displayKey: LocalizedStringKey {
        L.mode(self)
    }
```

The result around lines 9-25 should read:

```swift
    var displayName: String {
        switch self {
        case .active: return "Active"
        case .scanPaused: return "Scanning paused"
        case .monitoringDisabled: return "Monitoring disabled"
        }
    }

    /// Localized display name for views (`StatusChip`, picker labels).
    /// Routes through the catalog `mode.<rawValue>` keys.
    var displayKey: LocalizedStringKey {
        L.mode(self)
    }

    var symbolName: String {
        switch self {
        case .active: return "play.circle.fill"
        case .scanPaused: return "pause.circle.fill"
        case .monitoringDisabled: return "eye.slash.fill"
        }
    }
```

- [ ] **Step 2: Add SwiftUI import if missing**

The file currently imports `Foundation` and `Darwin`. `LocalizedStringKey` requires `SwiftUI`. Add at the top of the file (after `import Darwin`):

```swift
import SwiftUI
```

- [ ] **Step 3: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 4: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/JobStore.swift
git commit -m "feat(menubar): WatcherMode.displayKey for localized labels"
```

---

## Task 5: App.swift — locale environment + localized notification

**Files:**

- Modify: `macos-menubar/Sources/App/App.swift`

- [ ] **Step 1: Replace the file ENTIRELY with this content**

```swift
import SwiftUI
import UserNotifications

@main
struct FileSandboxMenuBarApp: App {
    @StateObject private var store = JobStore()
    @StateObject private var settingsStore = SettingsStore()
    @StateObject private var sandboxStore = SandboxStore()
    @State private var notifiedAtLaunch = false

    @AppStorage("filesandbox.locale") private var localeRaw: String = AppLocale.auto.rawValue

    init() {
        // UNUserNotificationCenter.current() asserts when there's no CFBundleIdentifier
        // (raw `swift run` from .build/). Skip request when running unbundled in dev.
        if Bundle.main.bundleIdentifier != nil {
            UNUserNotificationCenter.current().requestAuthorization(options: [.alert]) { _, _ in }
        }
    }

    private var appLocale: AppLocale {
        AppLocale(rawValue: localeRaw) ?? .auto
    }

    var body: some Scene {
        MenuBarExtra {
            MenuBarContentView(store: store, settingsStore: settingsStore, sandboxStore: sandboxStore)
                .environment(\.locale, resolvedLocale(for: appLocale) ?? Locale.current)
        } label: {
            Image(systemName: store.iconName)
                .symbolRenderingMode(.hierarchical)
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(menuBarIconColor(for: store.mode))
        }
        .menuBarExtraStyle(.window)
        .onChange(of: store.mode) { _, newMode in
            guard !notifiedAtLaunch else { return }
            notifiedAtLaunch = true
            guard newMode != .active else { return }
            postLaunchNotification(for: newMode)
        }
    }

    private func postLaunchNotification(for newMode: WatcherMode) {
        let locale = resolvedLocale(for: appLocale) ?? Locale.current
        let modeName: String = {
            switch newMode {
            case .active:              return String(localized: "mode.active",              locale: locale)
            case .scanPaused:          return String(localized: "mode.scan_paused",          locale: locale)
            case .monitoringDisabled:  return String(localized: "mode.monitoring_disabled",  locale: locale)
            }
        }()
        let titleFormat = String(localized: "FileSandbox started in %@", locale: locale)
        let bodyKey: String.LocalizationValue = newMode == .scanPaused
            ? "New files are quarantined but not scanned. Open the menu bar to resume."
            : "New files are not being monitored. Open the menu bar to resume."

        let content = UNMutableNotificationContent()
        content.title = String(format: titleFormat, modeName)
        content.body  = String(localized: bodyKey, locale: locale)

        let req = UNNotificationRequest(identifier: "filesandbox.launch.mode", content: content, trigger: nil)
        if Bundle.main.bundleIdentifier != nil {
            UNUserNotificationCenter.current().add(req)
        }
    }
}

private func menuBarIconColor(for mode: WatcherMode) -> Color {
    switch mode {
    case .active: return .primary
    case .scanPaused: return .orange
    case .monitoringDisabled: return .red
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

> If the compiler complains about `String.LocalizationValue` initialization from a string literal, the `bodyKey` line can be inlined into both `String(localized: ..., locale: locale)` calls instead of using the variable. SwiftUI's `String(localized:locale:)` accepts `String.LocalizationValue` which has `ExpressibleByStringLiteral` conformance.

- [ ] **Step 3: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/App.swift
git commit -m "feat(menubar): wire locale environment + localize launch notification"
```

---

## Task 6: StatusChip — use `displayKey`

**Files:**

- Modify: `macos-menubar/Sources/App/Components/StatusChip.swift`

- [ ] **Step 1: Update the chip to use `LocalizedStringKey`**

The current `label: String` property hard-codes `mode.displayName` (English text). Replace its computed type and value, and use `LocalizedStringKey` in the chip body.

Open the file. Find:

```swift
    private var label: String {
        isConnected ? mode.displayName : "Disconnected"
    }
```

Replace with:

```swift
    private var label: LocalizedStringKey {
        isConnected ? mode.displayKey : "Disconnected"
    }
```

Then in the body, the existing line `Text(label)` keeps working — `Text(_ key: LocalizedStringKey)` is the correct overload. No further changes.

Also update the menu items inside `Menu { ForEach(...) { Button { ... } label: { Label { Text(m.displayName) } icon: { ... } } } }`. Find:

```swift
                    Label {
                        Text(m.displayName)
                    } icon: {
```

Replace with:

```swift
                    Label {
                        Text(m.displayKey)
                    } icon: {
```

- [ ] **Step 2: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/Components/StatusChip.swift
git commit -m "feat(menubar): StatusChip uses LocalizedStringKey labels"
```

---

## Task 7: Tabs — `title` returns `LocalizedStringKey`

**Files:**

- Modify: `macos-menubar/Sources/App/Components/Tabs.swift`

- [ ] **Step 1: Change `AppTab.title` return type**

Find:

```swift
    var title: String {
        switch self {
        case .jobs:     return "Jobs"
        case .sandbox:  return "Sandbox"
        case .settings: return "Settings"
        }
    }
```

Replace with:

```swift
    var title: LocalizedStringKey {
        switch self {
        case .jobs:     return "Jobs"
        case .sandbox:  return "Sandbox"
        case .settings: return "Settings"
        }
    }
```

The body already calls `Text(tab.title)` — `Text(_ key: LocalizedStringKey)` overload now picks up the change automatically.

- [ ] **Step 2: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/Components/Tabs.swift
git commit -m "feat(menubar): AppTab.title returns LocalizedStringKey"
```

---

## Task 8: VerdictPill + SessionStatePill — route via `L` helpers

**Files:**

- Modify: `macos-menubar/Sources/App/Components/VerdictPill.swift`

- [ ] **Step 1: Change `VerdictPill.text` to `LocalizedStringKey`**

Find:

```swift
struct VerdictPill: View {
    enum Size { case mini, big }
    enum Variant { case red, orange, green, blue, grey }

    let text: String
    let variant: Variant
    let size: Size
    var symbol: String? = nil
```

Replace `let text: String` with `let text: LocalizedStringKey`. The result:

```swift
struct VerdictPill: View {
    enum Size { case mini, big }
    enum Variant { case red, orange, green, blue, grey }

    let text: LocalizedStringKey
    let variant: Variant
    let size: Size
    var symbol: String? = nil
```

Inside `body`, the existing `Text(text)` call already compiles against `LocalizedStringKey` — no body change.

- [ ] **Step 2: Update `forJobVerdict` to use `L.verdict`**

Find the entire `static func forJobVerdict(verdict:status:) -> VerdictPill?` extension. Replace the body with:

```swift
extension VerdictPill {
    /// Map a job's `vt_verdict` string + status to a pill variant + label.
    static func forJobVerdict(verdict: String?, status: String) -> VerdictPill? {
        if status == "scanning" || status == "received" {
            return VerdictPill(text: L.verdict("scanning"), variant: .blue, size: .mini, symbol: "hourglass")
        }
        if status == "in_quarantine" {
            return VerdictPill(text: L.verdict("queued"), variant: .blue, size: .mini, symbol: "tray")
        }
        guard let v = verdict?.lowercased() else { return nil }
        switch v {
        case "infected", "malicious":
            return VerdictPill(text: L.verdict(v), variant: .red, size: .mini, symbol: "exclamationmark.triangle.fill")
        case "inconclusive", "unclear":
            return VerdictPill(text: L.verdict("inconclusive"), variant: .orange, size: .mini, symbol: "questionmark.circle.fill")
        case "oversized":
            return VerdictPill(text: L.verdict("oversized"), variant: .grey, size: .mini, symbol: "arrow.down.circle")
        case "clean":
            return VerdictPill(text: L.verdict("clean"), variant: .green, size: .mini, symbol: "checkmark.circle.fill")
        default:
            return VerdictPill(text: LocalizedStringKey(v), variant: .grey, size: .mini)
        }
    }
}
```

- [ ] **Step 3: Update `SessionStatePill` to use `L.session`**

Replace the entire `SessionStatePill` body with:

```swift
struct SessionStatePill: View {
    let status: String
    var body: some View {
        switch status {
        case "running":   VerdictPill(text: L.session("running"),   variant: .green, size: .mini)
        case "starting":  VerdictPill(text: L.session("starting"),  variant: .blue,  size: .mini)
        case "stopped":   VerdictPill(text: L.session("stopped"),   variant: .red,   size: .mini)
        case "failed":    VerdictPill(text: L.session("failed"),    variant: .red,   size: .mini)
        case "discarded": VerdictPill(text: L.session("discarded"), variant: .grey,  size: .mini)
        default:          EmptyView()
        }
    }
}
```

- [ ] **Step 4: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 5: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/Components/VerdictPill.swift
git commit -m "feat(menubar): VerdictPill + SessionStatePill route via L helpers"
```

---

## Task 9: JobsTabView — `bigVerdictPill` via `L.verdictBig`

**Files:**

- Modify: `macos-menubar/Sources/App/Tabs/JobsTabView.swift`

- [ ] **Step 1: Replace the `bigVerdictPill()` private method body**

Find the existing helper (inside `private struct JobRow`):

```swift
    private func bigVerdictPill() -> VerdictPill? {
        let v = (job.vt_verdict ?? "").lowercased()
        switch v {
        case "infected", "malicious":
            return VerdictPill(text: "Infected", variant: .red, size: .big, symbol: "exclamationmark.triangle.fill")
        case "inconclusive":
            return VerdictPill(text: "Inconclusive", variant: .orange, size: .big, symbol: "questionmark.circle.fill")
        case "oversized":
            return VerdictPill(text: "Oversized", variant: .grey, size: .big, symbol: "arrow.down.circle")
        case "clean":
            return VerdictPill(text: "Clean", variant: .green, size: .big, symbol: "checkmark.circle.fill")
        default: return nil
        }
    }
```

Replace with:

```swift
    private func bigVerdictPill() -> VerdictPill? {
        let v = (job.vt_verdict ?? "").lowercased()
        switch v {
        case "infected", "malicious":
            return VerdictPill(text: L.verdictBig(v), variant: .red, size: .big, symbol: "exclamationmark.triangle.fill")
        case "inconclusive":
            return VerdictPill(text: L.verdictBig("inconclusive"), variant: .orange, size: .big, symbol: "questionmark.circle.fill")
        case "oversized":
            return VerdictPill(text: L.verdictBig("oversized"), variant: .grey, size: .big, symbol: "arrow.down.circle")
        case "clean":
            return VerdictPill(text: L.verdictBig("clean"), variant: .green, size: .big, symbol: "checkmark.circle.fill")
        default: return nil
        }
    }
```

> The static `Text("...")` calls elsewhere in this file (`Daemon offline`, `Start daemon`, `Open in sandbox`, `Restore`, `Delete`, group titles, empty-state copy, the `Watching %@` placeholder) all pick up translations automatically because `Text(_ key: LocalizedStringKey)` is invoked when a literal string is passed. No further changes here.

- [ ] **Step 2: Update `EngineCard`'s value fallback**

Inside the same file's `expandedDetail` view, find:

```swift
                EngineCard(
                    label: "VirusTotal",
                    value: job.vt_verdict ?? "-",
                    status: engineStatus(for: job.vt_verdict)
                )
```

The label `"VirusTotal"` is a brand and stays English. The fallback `"-"` is a single ASCII hyphen with no translation needed. No change required here. Move on.

- [ ] **Step 3: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 4: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/Tabs/JobsTabView.swift
git commit -m "feat(menubar): JobsTabView bigVerdictPill via L.verdictBig"
```

---

## Task 10: SandboxTabView — `IconButton.help` typed as `LocalizedStringKey`

**Files:**

- Modify: `macos-menubar/Sources/App/Tabs/SandboxTabView.swift`

- [ ] **Step 1: Change the `IconButton` help parameter type**

Find:

```swift
private struct IconButton: View {
    let symbol: String
    let help: String
    var isDanger: Bool = false
    let action: () -> Void
```

Replace with:

```swift
private struct IconButton: View {
    let symbol: String
    let help: LocalizedStringKey
    var isDanger: Bool = false
    let action: () -> Void
```

In the same file find:

```swift
        .help(help)
```

This line stays as-is — `.help(_:)` accepts `LocalizedStringKey`.

- [ ] **Step 2: Verify call sites still compile**

The three IconButton call sites inside `SandboxRowView` already pass string literals:

```swift
            IconButton(symbol: "plus.viewfinder", help: "Show window") { ... }
            IconButton(symbol: "square.and.arrow.up", help: "Export") { ... }
            IconButton(symbol: "xmark.circle", help: "Discard", isDanger: true, action: onDiscard)
```

String literals satisfy `LocalizedStringKey` via `ExpressibleByStringLiteral`. No call-site change.

- [ ] **Step 3: Update the top-strip `Button` help text on `+ New session`**

Find:

```swift
            .help(store.canOpen ? "Pick a file and open it in a fresh sandbox VM" : "Install Tart to enable")
```

This already passes string literals. `.help(_:)` resolves the literal as `LocalizedStringKey` — both keys are in the catalog. No change needed.

- [ ] **Step 4: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 5: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/Tabs/SandboxTabView.swift
git commit -m "feat(menubar): IconButton.help typed as LocalizedStringKey"
```

---

## Task 11: SettingsTabView — Language picker row in Advanced

**Files:**

- Modify: `macos-menubar/Sources/App/Tabs/SettingsTabView.swift`

- [ ] **Step 1: Add `@AppStorage` binding to the View**

Find the existing struct properties:

```swift
struct SettingsTabView: View {
    @ObservedObject var settingsStore: SettingsStore
    @ObservedObject var store: JobStore

    /// Debounced auto-save: any @Published change triggers `save()` 400 ms later.
    @State private var saveTimer: AnyCancellable? = nil
```

Add the `@AppStorage` line after `store`:

```swift
struct SettingsTabView: View {
    @ObservedObject var settingsStore: SettingsStore
    @ObservedObject var store: JobStore
    @AppStorage("filesandbox.locale") private var localeRaw: String = AppLocale.auto.rawValue

    /// Debounced auto-save: any @Published change triggers `save()` 400 ms later.
    @State private var saveTimer: AnyCancellable? = nil
```

- [ ] **Step 2: Add the Language row inside the Advanced group**

Find the last row of the Advanced group, the `VT API key` SecureField row:

```swift
                SettingRow(label: "VT API key") {
                    SecureField("", text: $settingsStore.vtApiKey, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }
            }
        }
```

Immediately AFTER this `SettingRow` block and BEFORE the closing `}` that ends the inner VStack, insert the Language row:

```swift
                SettingRow(label: "Language") {
                    Picker("", selection: $localeRaw) {
                        ForEach(AppLocale.allCases) { loc in
                            Text(loc.displayName).tag(loc.rawValue)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 180)
                    .labelsHidden()
                }
```

The result for the tail of Advanced:

```swift
                SettingRow(label: "VT API key") {
                    SecureField("", text: $settingsStore.vtApiKey, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }
                SettingRow(label: "Language") {
                    Picker("", selection: $localeRaw) {
                        ForEach(AppLocale.allCases) { loc in
                            Text(loc.displayName).tag(loc.rawValue)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 180)
                    .labelsHidden()
                }
            }
        }
```

> The `Language` change does NOT call `scheduleSave()` — it's a pure-UI preference, not a daemon setting. The `@AppStorage` write persists locally and the `.environment(\.locale, ...)` modifier in `App.swift` re-evaluates immediately.

- [ ] **Step 3: Verify build**

Run: `cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 4: Commit**

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add macos-menubar/Sources/App/Tabs/SettingsTabView.swift
git commit -m "feat(menubar): Settings → Advanced → Language picker"
```

---

## Task 12: Manual acceptance run

**Files:** none.

This task does not write code. It validates the full localization end-to-end against the spec's Acceptance checklist.

- [ ] **Step 1: Build the bundled `.app`**

```bash
cd /Users/artemmac/dev/personal/file-sandbox/macos-menubar
./build.sh
```
Expected last lines:
```
Build complete! (...s)
Done: /Users/artemmac/dev/personal/file-sandbox/macos-menubar/FileSandboxMenuBar.app
Run: open "/Users/artemmac/dev/personal/file-sandbox/macos-menubar/FileSandboxMenuBar.app"
```

- [ ] **Step 2: Stop any running instance and relaunch**

```bash
pkill -f FileSandboxMenuBar
open /Users/artemmac/dev/personal/file-sandbox/macos-menubar/FileSandboxMenuBar.app
```

- [ ] **Step 3: Walk the spec acceptance checklist**

In the menu-bar dropdown, verify each item from § Acceptance checklist of `docs/superpowers/specs/2026-05-04-russian-localization-design.md`:

1. First launch: UI matches system locale (English on en-system, Russian on ru-system).
2. Settings → Advanced → Language → English → header shows `Disconnected`/`Active`, tabs `Jobs`/`Sandbox`/`Settings`, footer `Restart daemon · View logs` `Quit`.
3. Settings → Advanced → Language → Русский → header `Не подключено`/`Активно`, tabs `Задачи`/`Песочница`/`Настройки`, footer `Перезапустить демон · Открыть логи` `Выход`.
4. Switching is instant; no relaunch needed; tabs/expanded rows/Settings re-render in the new language.
5. With a quarantine-kept job in DB, expanded row shows `Заражён` (big) and `заражён` (mini) on Russian.
6. Sandbox row state pills show `работает` / `запускается` on Russian.
7. Mode chip menu items: `Активно` / `Сканирование на паузе` / `Мониторинг выключен` on Russian.
8. Restart the app with `localeRaw=ru` set → language is preserved.
9. Launch with `watcherMode=scan_paused` → notification body is in the chosen language.
10. `VirusTotal`, `pompelmi`, `Tart`, `clamd`, `MiB`, `min`, `days`, `Bearer`, `fsbx-XXXXXXXX`, paths stay English in both languages.
11. Unknown verdict (e.g. `error`) renders as raw `error` (or its catalog miss key like `verdict.error`) without crashing.

- [ ] **Step 4: Stop and finalize**

```bash
pkill -f FileSandboxMenuBar || true
```

If everything passed, no follow-up commit needed. If a regression was found, fix it in a small follow-up commit:

```bash
cd /Users/artemmac/dev/personal/file-sandbox
git add <fix files>
git commit -m "fix(menubar-l10n): <regression caught in acceptance>"
```

---

## Self-review

**1. Spec coverage** — every spec section maps to a task:

| Spec section | Implemented in |
|---|---|
| Architecture / `AppLocale` / `resolvedLocale` / `enum L` | Task 3 |
| `defaultLocalization` + `Resources` | Task 1 |
| `Localizable.xcstrings` (full table) | Task 2 |
| `@AppStorage("filesandbox.locale")` + `.environment(\.locale)` | Task 5 |
| Localized launch notification | Task 5 |
| `WatcherMode.displayKey` (Open work item from spec) | Task 4 (kept `displayName: String` for non-localized contexts as recommended) |
| `StatusChip` uses `displayKey` + `Disconnected` natural key | Task 6 |
| `AppTab.title` returns `LocalizedStringKey` | Task 7 |
| `VerdictPill.text` typed `LocalizedStringKey` + `forJobVerdict` via `L.verdict` | Task 8 |
| `SessionStatePill` via `L.session` | Task 8 |
| `bigVerdictPill()` via `L.verdictBig` | Task 9 |
| `IconButton.help` typed `LocalizedStringKey` | Task 10 |
| Settings → Advanced → Language picker bound to `@AppStorage` | Task 11 |
| Acceptance walkthrough | Task 12 |

**2. Placeholder scan** — all `TBD/TODO/implement later` patterns absent. Code blocks complete in every step.

**3. Type consistency:**
- `AppLocale.displayName: LocalizedStringKey` in Task 3 → consumed in Task 11 Picker. ✓
- `enum L { static func verdict / verdictBig / session / mode }` signatures used identically in Tasks 4, 8, 9. ✓
- `WatcherMode.displayKey: LocalizedStringKey` in Task 4 → consumed in Task 6 (StatusChip). ✓
- `IconButton.help: LocalizedStringKey` in Task 10 → call sites already pass string literals (auto-conform). ✓

**4. Open work from spec** — `WatcherMode.displayName` flip resolved: kept `displayName: String` for `String(localized:)` use in notification (Task 5), added new `displayKey: LocalizedStringKey` for view layer (Task 4). Both coexist; no breaking API change.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-04-russian-localization.md`. Two execution options:

1. **Subagent-Driven (recommended)** — fresh subagent per task, two-stage review, fast iteration.
2. **Inline Execution** — batch execution with checkpoints in this session.

Which approach?
