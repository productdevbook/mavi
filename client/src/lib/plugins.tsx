/* eslint-disable react-refresh/only-export-components -- provider + hook share one module */
import * as React from "react"

import { setActivePluginSnapshot } from "@api"
import type { PluginRecord } from "@api"

import { api } from "@/lib/api"

export const PLUGIN_IDS = [
  "core",
  "writing",
  "commerce",
  "learning",
  "forms",
  "messaging",
  "automation",
  "boards",
  "analytics",
  "governance",
] as const

export type PluginId = (typeof PLUGIN_IDS)[number]

export interface PluginState {
  /** The catalog is only loaded by the management screen. */
  records: PluginRecord[] | null
  /** Runtime truth used by the shell, sourced from the public manifest. */
  activePlugins: ReadonlySet<PluginId>
  ready: boolean
  loading: boolean
  error: unknown | null
  refresh: (loadCatalog?: boolean) => Promise<void>
  enable: (id: PluginId) => Promise<void>
  disable: (id: PluginId) => Promise<void>
}

const PluginContext = React.createContext<PluginState | null>(null)

function knownPlugin(value: string): value is PluginId {
  return (PLUGIN_IDS as readonly string[]).includes(value)
}

export const isPluginId = knownPlugin

/** Loads only the active compiled feature set into the authenticated shell. */
export function PluginProvider({ children }: { children: React.ReactNode }) {
  const [records, setRecords] = React.useState<PluginRecord[] | null>(null)
  const [activePlugins, setActivePlugins] = React.useState<ReadonlySet<PluginId>>(
    new Set(),
  )
  const [ready, setReady] = React.useState(false)
  const [loading, setLoading] = React.useState(false)
  const [error, setError] = React.useState<unknown | null>(null)

  const refresh = React.useCallback(async (loadCatalog = false) => {
    setLoading(true)
    setError(null)

    let manifestLoaded = false
    try {
      const manifest = await api("runtime.manifest.read")
      manifestLoaded = true
      const active = new Set<PluginId>()
      for (const id of manifest.active_plugins) {
        if (knownPlugin(id)) active.add(id)
      }
      setActivePlugins(active)
      setActivePluginSnapshot(active)

      if (loadCatalog) {
        setRecords(await api("plugins.list"))
      }
      setReady(true)
    } catch (why) {
      setError(why)
      // A failed manifest must not optimistically render product routes. A
      // catalog permission failure, however, must not make an already-known
      // active feature disappear from somebody's sidebar.
      if (!manifestLoaded) {
        const empty = new Set<PluginId>()
        setActivePlugins(empty)
        setActivePluginSnapshot(empty)
      }
      setReady(true)
      if (loadCatalog) setRecords([])
      throw why
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void refresh().catch(() => undefined)
  }, [refresh])

  const enable = React.useCallback(
    async (id: PluginId) => {
      await api("plugins.enable", { path: { id } })
      await refresh(true)
    },
    [refresh],
  )

  const disable = React.useCallback(
    async (id: PluginId) => {
      await api("plugins.disable", { path: { id } })
      await refresh(true)
    },
    [refresh],
  )

  const value = React.useMemo<PluginState>(
    () => ({
      records,
      activePlugins,
      ready,
      loading,
      error,
      refresh,
      enable,
      disable,
    }),
    [records, activePlugins, ready, loading, error, refresh, enable, disable],
  )

  return <PluginContext.Provider value={value}>{children}</PluginContext.Provider>
}

export function usePlugins(): PluginState {
  const value = React.useContext(PluginContext)
  if (!value) {
    return {
      records: null,
      activePlugins: new Set(),
      ready: false,
      loading: false,
      error: null,
      refresh: async () => undefined,
      enable: async () => undefined,
      disable: async () => undefined,
    }
  }
  return value
}
