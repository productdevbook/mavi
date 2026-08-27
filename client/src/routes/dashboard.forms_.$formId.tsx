/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const FormDetailPage = lazyRoute(() =>
  import("@/features/forms/form-detail-page").then(({ FormDetailPage }) => ({
    default: FormDetailPage,
  })),
)

export const Route = createFileRoute("/dashboard/forms_/$formId")({
  component: FormDetailRoute,
})

function FormDetailRoute() {
  const { formId } = Route.useParams()

  return <FormDetailPage formId={formId} />
}
