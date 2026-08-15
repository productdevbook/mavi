import { drawing, type Entry } from "@/lib/v1-audit"

/**
 * What was done to this site, drawn once.
 *
 * A phrase and how grave it looks belong together: written in two places they
 * drift, and what drifts is which entries somebody's eye stops on.
 */
export function AuditTable({ entries }: { entries: Entry[] }) {
  return (
    <div className="overflow-x-auto rounded-xl border border-border">
      <table className="w-full text-sm">
        <tbody>
          {entries.map((entry) => {
            const { icon: Icon, grave } = drawing(entry.action)

            return (
              <tr
                key={entry.id}
                className="border-b border-border last:border-0"
              >
                <td className="w-10 py-2 pl-3 align-top">
                  <Icon
                    className={
                      grave
                        ? "size-4 text-destructive"
                        : "size-4 text-muted-foreground"
                    }
                  />
                </td>
                <td className="py-2 align-top">
                  <p className="font-medium">{entry.action}</p>
                  <p className="text-muted-foreground">
                    {entry.subject}
                    {entry.subject_id ? ` \u00b7 ${entry.subject_id}` : ""}
                  </p>
                </td>
                <td className="py-2 align-top">
                  {entry.actor_name ?? entry.actor_kind}
                </td>
                <td className="py-2 pr-3 text-right align-top whitespace-nowrap text-muted-foreground">
                  {new Date(entry.created_at).toLocaleString()}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
