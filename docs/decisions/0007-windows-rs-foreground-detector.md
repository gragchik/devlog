# ADR-0007: Win32 foreground-detector — крейт `windows` (замена ADR-0003)

**Статус:** принято и проверено (Итерация 2, Tauri).

## Контекст

Старый план (ADR-0003, archived) рассматривал `koffi` — FFI-библиотеку для
Node/Electron, нужную именно потому, что JS не может напрямую звать Win32
API. В Rust этой проблемы нет: крейт `windows` (`windows-rs`) — официальные,
типизированные биндинги Microsoft ко всему Win32 API, без отдельного FFI-слоя.

## Решение

`windows` крейт, с точечно включёнными feature-флагами по мере
необходимости (`Win32_UI_WindowsAndMessaging`, `Win32_System_Threading`,
`Win32_UI_Input_KeyboardAndMouse`, `Win32_System_StationsAndDesktops`,
`Win32_System_SystemInformation`) — не весь API целиком, чтобы не раздувать
время компиляции.

Реализовано в `src-tauri/src/platform/windows_activity_adapter.rs`:

- `get_foreground_process_name()` — `GetForegroundWindow` →
  `GetWindowThreadProcessId` → `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`
  → `QueryFullProcessImageNameW` → берём basename.
- `get_system_idle_seconds()` — `GetLastInputInfo` + `GetTickCount64`.
- `is_session_locked(foreground_process_name)` — **два** сигнала вместе
  (см. "Находка" ниже).

## Находка при ручной проверке на реальной машине

Изначальная версия `is_session_locked()` проверяла только
`OpenInputDesktop`+`GetUserObjectInformationW` (имя input desktop ≠
"Default" = заблокировано) — классическая техника, которая работает для
UAC-промптов и экрана ввода пароля. Но ручной smoke-test
(`cargo test manual_smoke -- --ignored --nocapture`) на реальной
заблокированной сессии показал:

```
foreground process: Some("LockApp.exe")
idle seconds: Some(592)
is session locked: false   ← неверно!
```

Причина: начиная с Windows 8.1+, современный экран блокировки (тот, что со
свайпом/фоновым фото) — это UWP-приложение `LockApp.exe`, рендерящееся
**поверх обычного "Default" input desktop**. Настоящее переключение на
защищённый Winlogon-desktop происходит только в момент, когда пользователь
начинает вводить пароль — то есть чисто desktop-switch-проверка пропускает
сам факт блокировки, пока не начат ввод.

**Исправление:** `is_session_locked` дополнительно проверяет, не является
ли foreground-процесс сам экраном блокировки/входа (`LockApp.exe`,
`LogonUI.exe`). Повторный smoke-test на разблокированной сессии (foreground
= `chrome.exe`) корректно дал `is session locked: false`.

## Проверено практически

- Компилируется и линкуется (crates.io `windows` v0.62.2, уже была
  подтянута транзитивно через сам Tauri/wry — отсюда дополнительная выгода:
  нет дублирования версий в дереве зависимостей).
- Ручной smoke-test на реальной Windows-сессии — оба сценария
  (заблокировано/разблокировано) дали верный результат после фикса.
- Сквозной тест через настоящий `npm run dev`: реальные `activity_events`
  накапливаются каждые 5 секунд, foreground-процесс и idle-секунды меняются
  корректно во времени (см. `docs/iterations/02.md`).

## Остаточные ограничения (осознанно, не блокируют MVP)

- `is_session_locked` не проверено на Fast User Switching (переключение
  между несколькими пользователями Windows) — отдельный сценарий, не
  входит в acceptance Итерации 2.
- Suspend/resume (сон/пробуждение ноутбука) определяется **косвенно** —
  через разрыв между poll (`SUSPEND_GAP_MULTIPLIER`), а не через
  `PowerRegisterSuspendResumeNotification`. Это сознательное упрощение
  (см. `docs/iterations/02.md`, "Сознательно не сделано") — рассмотреть
  переход на нативный callback, если разрыв-эвристика даст ложные
  срабатывания на практике (например, из-за высокой нагрузки CPU, тормозящей
  poll-поток без реального сна системы).
