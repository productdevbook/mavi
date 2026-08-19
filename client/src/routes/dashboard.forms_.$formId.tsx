/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { FormDetailPage } from "@/features/forms/form-detail-page"

export const Route = createFileRoute("/dashboard/forms_/$formId")({
  component: FormDetailRoute,
})

function FormDetailRoute() {
  const { formId } = Route.useParams()

  return <FormDetailPage formId={formId} />
}
