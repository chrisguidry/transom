# How GitHub Actions deploys the gateway: a workflow on main trades its
# OIDC token through Workload Identity Federation for the deployer
# service account, which may deploy this one service and nothing else.
# No key leaves Google.

resource "google_iam_workload_identity_pool" "github" {
  project                   = google_project.transom.project_id
  workload_identity_pool_id = "github"
  display_name              = "GitHub Actions"

  depends_on = [google_project_service.transom]
}

resource "google_iam_workload_identity_pool_provider" "github" {
  project                            = google_project.transom.project_id
  workload_identity_pool_id          = google_iam_workload_identity_pool.github.workload_identity_pool_id
  workload_identity_pool_provider_id = "transom"
  display_name                       = "transom"

  attribute_mapping = {
    "google.subject"       = "assertion.sub"
    "attribute.repository" = "assertion.repository"
    "attribute.ref"        = "assertion.ref"
  }

  # Only main deploys, so a pull request's workflow cannot take the
  # deployer's place.
  attribute_condition = join(" && ", [
    "assertion.repository == '${var.github_repository}'",
    "assertion.repository_id == '${var.github_repository_id}'",
    "assertion.ref == 'refs/heads/main'",
  ])

  oidc {
    issuer_uri = "https://token.actions.githubusercontent.com"
  }
}

resource "google_service_account" "deployer" {
  project      = google_project.transom.project_id
  account_id   = "deployer"
  display_name = "transom deployer"
  description  = "Deploys the gateway from GitHub Actions"

  depends_on = [google_project_service.transom]
}

resource "google_service_account_iam_member" "github_acts_as_deployer" {
  service_account_id = google_service_account.deployer.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "principalSet://iam.googleapis.com/${google_iam_workload_identity_pool.github.name}/attribute.repository/${var.github_repository}"
}

resource "google_cloud_run_v2_service_iam_member" "deployer_deploys_gateway" {
  project  = google_cloud_run_v2_service.gateway.project
  location = google_cloud_run_v2_service.gateway.location
  name     = google_cloud_run_v2_service.gateway.name
  role     = "roles/run.developer"
  member   = google_service_account.deployer.member
}

# A new revision runs as the gateway's service account, and Cloud Run
# lets only someone who may act as that account deploy one.
resource "google_service_account_iam_member" "deployer_acts_as_gateway" {
  service_account_id = google_service_account.gateway.name
  role               = "roles/iam.serviceAccountUser"
  member             = google_service_account.deployer.member
}
