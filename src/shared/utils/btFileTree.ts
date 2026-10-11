/** @fileoverview Folder tree and type groups for the BitTorrent file selector. */
import { BUILTIN_CATEGORY_TEMPLATES } from '@shared/constants'
import type { BtFileSelectionItem } from '@shared/types'

export interface BtFileTreeRow {
  /** File index for files; `dir:<path>` for folders. */
  key: number | string
  name: string
  path: string
  length: number
  index?: number
  children?: BtFileTreeRow[]
}

export type BtFileType = (typeof BUILTIN_CATEGORY_TEMPLATES)[number]['label']

function segments(path: string): string[] {
  return path.split(/[/\\]+/).filter(Boolean)
}

/** Number of leading folders every file shares; a magnet's files carry the absolute save path. */
function sharedDepth(paths: string[][]): number {
  if (paths.length === 0) return 0
  let depth = Math.min(...paths.map((parts) => parts.length - 1))
  for (const parts of paths) {
    for (let i = 0; i < depth; i++) {
      if (parts[i] !== paths[0][i]) {
        depth = i
        break
      }
    }
  }
  return Math.max(depth, 0)
}

/** Merges folders that only hold another folder, so deep paths take one row. */
function compact(rows: BtFileTreeRow[]): BtFileTreeRow[] {
  return rows.map((row) => {
    let folder = row
    while (folder.children?.length === 1 && folder.children[0].children) {
      const child = folder.children[0]
      folder = { ...child, name: `${folder.name}/${child.name}` }
    }
    return folder.children ? { ...folder, children: compact(folder.children) } : folder
  })
}

/** Folders first, each level in torrent order. */
function sortFoldersFirst(rows: BtFileTreeRow[]): BtFileTreeRow[] {
  const folders = rows.filter((row) => row.children)
  const files = rows.filter((row) => !row.children)
  return [...folders.map((row) => ({ ...row, children: sortFoldersFirst(row.children ?? []) })), ...files]
}

/** Builds the folder tree below the folders all files share; folder sizes sum their files. */
export function buildBtFileTree(files: BtFileSelectionItem[]): BtFileTreeRow[] {
  const paths = files.map((file) => segments(file.path))
  const depth = sharedDepth(paths)
  const root: BtFileTreeRow[] = []
  const folders = new Map<string, BtFileTreeRow>()
  files.forEach((file, position) => {
    const parts = paths[position].slice(depth)
    let siblings = root
    let prefix = ''
    for (const segment of parts.slice(0, -1)) {
      prefix = prefix ? `${prefix}/${segment}` : segment
      let folder = folders.get(prefix)
      if (!folder) {
        folder = { key: `dir:${prefix}`, name: segment, path: prefix, length: 0, children: [] }
        folders.set(prefix, folder)
        siblings.push(folder)
      }
      folder.length += file.length
      siblings = folder.children as BtFileTreeRow[]
    }
    siblings.push({
      key: file.index,
      name: parts[parts.length - 1] ?? file.name,
      path: file.path,
      length: file.length,
      index: file.index,
    })
  })
  return sortFoldersFirst(compact(root))
}

/** Built-in category of a file name, or null when no category lists its extension. */
export function btFileType(name: string): BtFileType | null {
  const dot = name.lastIndexOf('.')
  if (dot <= 0) return null
  const extension = name.slice(dot + 1).toLowerCase()
  return (
    BUILTIN_CATEGORY_TEMPLATES.find((template) => (template.extensions as readonly string[]).includes(extension))
      ?.label ?? null
  )
}

/** File indices per category, in category order; empty categories are left out. */
export function groupBtFilesByType(files: BtFileSelectionItem[]): { type: BtFileType; indices: number[] }[] {
  const groups = new Map<BtFileType, number[]>()
  for (const file of files) {
    const type = btFileType(file.name)
    if (!type) continue
    const group = groups.get(type)
    if (group) group.push(file.index)
    else groups.set(type, [file.index])
  }
  return BUILTIN_CATEGORY_TEMPLATES.filter((template) => groups.has(template.label)).map((template) => ({
    type: template.label,
    indices: groups.get(template.label) ?? [],
  }))
}

/** Selects every index of a group, or clears the group when it is already fully selected. */
export function toggleBtFileGroup(selected: number[], group: number[]): number[] {
  const current = new Set(selected)
  if (group.every((index) => current.has(index))) {
    for (const index of group) current.delete(index)
  } else {
    for (const index of group) current.add(index)
  }
  return [...current].sort((a, b) => a - b)
}

/** The files that are not selected. */
export function invertBtFileSelection(files: BtFileSelectionItem[], selected: number[]): number[] {
  const current = new Set(selected)
  return files.map((file) => file.index).filter((index) => !current.has(index))
}
