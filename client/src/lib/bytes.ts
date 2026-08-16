/** Bytes as a person says them. */
export function inBytes(bytes: number): string {
  const units = ["B", "kB", "MB", "GB", "TB"]
  let size = bytes
  let unit = 0
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024
    unit += 1
  }
  // One decimal below a gigabyte is noise; above it, it is the difference
  // between two bills.
  return `${size.toFixed(unit >= 3 ? 1 : 0)} ${units[unit]}`
}
