export type PasswordStrength = "weak" | "medium" | "strong"

export function passwordStrength(password: string): PasswordStrength {
  if (password.length < 8) return "weak"

  let variety = 0
  if (/[a-z]/.test(password)) variety++
  if (/[A-Z]/.test(password)) variety++
  if (/[0-9]/.test(password)) variety++
  if (/[^a-zA-Z0-9]/.test(password)) variety++

  if (password.length >= 12 && variety >= 3) return "strong"
  if (password.length >= 8 && variety >= 2) return "medium"
  return "weak"
}

const PASSWORD_CHARS =
  "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%^&*"

export function generatePassword(length = 16): string {
  const bytes = new Uint32Array(length)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, (byte) => PASSWORD_CHARS[byte % PASSWORD_CHARS.length]).join("")
}
