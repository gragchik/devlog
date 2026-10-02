import { describe, expect, it } from 'vitest'
import { secondsToHms } from '@shared/utils/time'

describe('secondsToHms', () => {
  it('форматирует нулевые секунды', () => {
    expect(secondsToHms(0)).toBe('0:00:00')
  })

  it('форматирует минуты и секунды с ведущими нулями', () => {
    expect(secondsToHms(65)).toBe('0:01:05')
  })

  it('форматирует часы больше 9', () => {
    expect(secondsToHms(36_000)).toBe('10:00:00')
  })

  it('отбрасывает дробную часть секунды', () => {
    expect(secondsToHms(59.9)).toBe('0:00:59')
  })

  it('выбрасывает ошибку на отрицательном значении', () => {
    expect(() => secondsToHms(-1)).toThrow(RangeError)
  })

  it('выбрасывает ошибку на NaN/Infinity', () => {
    expect(() => secondsToHms(Number.NaN)).toThrow(RangeError)
    expect(() => secondsToHms(Number.POSITIVE_INFINITY)).toThrow(RangeError)
  })
})
