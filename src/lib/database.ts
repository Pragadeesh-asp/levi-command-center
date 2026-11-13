import Database from '@tauri-apps/plugin-sql'

let connection: Promise<Database> | undefined

export function getDatabase(): Promise<Database> {
  connection ??= Database.load('sqlite:command-center.db')
  return connection
}

export const newId = () => crypto.randomUUID()

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}
