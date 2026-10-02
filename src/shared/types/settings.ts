export interface WhitelistEntry {
  processName: string
  category: string
}

export interface TrackerThresholds {
  idleThresholdSeconds: number
  pollIntervalSeconds: number
}
