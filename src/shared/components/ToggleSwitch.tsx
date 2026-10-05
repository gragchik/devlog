/**
 * Переключатель вкл/выкл ("Активная сессия") — используется и в
 * main-window (Dashboard), и в overlay (один и тот же сервис на оба
 * окна, ТЗ раздел 6). Обёртка над `<input type="checkbox">`, а не
 * кастомная кнопка — сохраняет доступность (пробел/клик по лейблу,
 * нативное `:checked`-состояние) без лишней ARIA-разметки.
 */
export function ToggleSwitch({
  checked,
  onChange,
  disabled = false,
  label,
  size = 'md'
}: {
  checked: boolean
  onChange: (next: boolean) => void
  disabled?: boolean
  label?: string
  size?: 'md' | 'sm'
}): JSX.Element {
  return (
    <label className={`toggle-switch toggle-switch-${size} ${disabled ? 'disabled' : ''}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="toggle-switch-track" aria-hidden="true" />
      {label && <span className="toggle-switch-label">{label}</span>}
    </label>
  )
}
