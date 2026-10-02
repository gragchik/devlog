export interface Project {
  id: string
  name: string
  repoPath: string
  enabled: boolean
  issueRegex: string
  preferences: Record<string, unknown>
  createdAtUtc: number
  updatedAtUtc: number
}

export interface CreateProjectInput {
  name: string
  repoPath: string
  enabled?: boolean
  issueRegex?: string
  preferences?: Record<string, unknown>
}

export interface UpdateProjectInput {
  name?: string
  enabled?: boolean
  issueRegex?: string
  preferences?: Record<string, unknown>
}
