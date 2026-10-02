import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { SettingsRepository } from '../../src/main/db/repositories/settings-repository'
import { createTempDatabase, type TempDatabase } from './helpers/temp-database'

describe('SettingsRepository', () => {
  let temp: TempDatabase
  let repo: SettingsRepository

  beforeEach(() => {
    temp = createTempDatabase()
    repo = new SettingsRepository(temp.db)
  })

  afterEach(() => {
    temp.cleanup()
  })

  it('get возвращает null для неизвестного ключа', () => {
    expect(repo.get('missing')).toBeNull()
  })

  it('set/get сохраняют и перезаписывают значение (upsert)', () => {
    repo.set('idleThresholdSeconds', '180')
    expect(repo.get('idleThresholdSeconds')).toBe('180')
    repo.set('idleThresholdSeconds', '240')
    expect(repo.get('idleThresholdSeconds')).toBe('240')
  })

  it('getAll возвращает все ключи как объект', () => {
    repo.set('a', '1')
    repo.set('b', '2')
    expect(repo.getAll()).toEqual({ a: '1', b: '2' })
  })

  it('remove удаляет ключ', () => {
    repo.set('a', '1')
    repo.remove('a')
    expect(repo.get('a')).toBeNull()
  })
})
