/**
 * Only what a tree needs, so that anything shaped like a category — the API's
 * own term, or a screen's narrower view of one — can be arranged by this.
 */
export interface Category {
  id: string
  name: string
  parent_id?: string | null
}

/** A category with its depth in the tree, in the order it should be shown. */
export interface CategoryRow<T extends Category = Category> {
  category: T
  depth: number
}

/**
 * Flattens categories into display order, each parent immediately followed by
 * its children. A category whose parent is missing — filtered out by language,
 * or deleted — is treated as a root so it stays visible instead of vanishing
 * with its branch.
 */
export function toCategoryTree<T extends Category>(
  categories: T[],
): CategoryRow<T>[] {
  const byParent = new Map<string | null, T[]>()
  const ids = new Set(categories.map((category) => category.id))

  for (const category of categories) {
    const parent =
      category.parent_id && ids.has(category.parent_id)
        ? category.parent_id
        : null
    const siblings = byParent.get(parent) ?? []
    siblings.push(category)
    byParent.set(parent, siblings)
  }
  for (const siblings of byParent.values()) {
    siblings.sort((a, b) => a.name.localeCompare(b.name))
  }

  const rows: CategoryRow<T>[] = []
  const walk = (parent: string | null, depth: number) => {
    for (const category of byParent.get(parent) ?? []) {
      rows.push({ category, depth })
      walk(category.id, depth + 1)
    }
  }
  walk(null, 0)
  return rows
}

/** A category and everything under it — never a valid parent for itself. */
export function descendantsOf(
  id: string,
  categories: Category[],
): Set<string> {
  const blocked = new Set([id])
  let grew = true
  while (grew) {
    grew = false
    for (const category of categories) {
      if (
        category.parent_id &&
        blocked.has(category.parent_id) &&
        !blocked.has(category.id)
      ) {
        blocked.add(category.id)
        grew = true
      }
    }
  }
  return blocked
}
