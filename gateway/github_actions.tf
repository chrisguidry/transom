# How GitHub Actions reaches Google: a workflow trades its OIDC token
# through Workload Identity Federation for a service account, so no key
# leaves Google. A push to main may act as the deployer, which deploys
# the gateway's service and nothing else, or as terraform-apply, which
# applies this directory. A pull request may act only as terraform-plan,
# which reads.

locals {
  pool_principals = "principalSet://iam.googleapis.com/${google_iam_workload_identity_pool.github.name}"
  pushes_to_main  = "${local.pool_principals}/attribute.ref/refs/heads/main"
  pull_requests   = "${local.pool_principals}/attribute.event_name/pull_request"
}

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
    "attribute.event_name" = "assertion.event_name"
  }

  # Only this repository, and only a push to main or a pull request. A
  # pull request's ref is refs/pull/<number>/merge, so the bindings
  # below that name refs/heads/main admit only a push to main. Events
  # such as pull_request_target, whose ref is main though a pull request
  # started them, are refused here.
  attribute_condition = join(" && ", [
    "assertion.repository == '${var.github_repository}'",
    "assertion.repository_id == '${var.github_repository_id}'",
    "((assertion.event_name == 'push' && assertion.ref == 'refs/heads/main') || assertion.event_name == 'pull_request')",
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

resource "google_service_account_iam_member" "main_acts_as_deployer" {
  service_account_id = google_service_account.deployer.name
  role               = "roles/iam.workloadIdentityUser"
  member             = local.pushes_to_main
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

# terraform-plan reads everything this directory manages, for a pull
# request's plan, and changes nothing.
resource "google_service_account" "terraform_plan" {
  project      = google_project.transom.project_id
  account_id   = "terraform-plan"
  display_name = "transom Terraform plan"
  description  = "Plans gateway/ from a pull request's workflow"

  depends_on = [google_project_service.transom]
}

resource "google_service_account_iam_member" "pull_requests_act_as_terraform_plan" {
  service_account_id = google_service_account.terraform_plan.name
  role               = "roles/iam.workloadIdentityUser"
  member             = local.pull_requests
}

resource "google_project_iam_member" "terraform_plan" {
  project = google_project.transom.project_id
  member  = google_service_account.terraform_plan.member

  for_each = toset([
    "roles/viewer",
    # The providers bill each call's quota to the project.
    "roles/serviceusage.serviceUsageConsumer",
    # The IAM policies of the service accounts and the Cloud Run service.
    "roles/iam.securityReviewer",
    # The Android app's configuration, which the outputs read.
    "roles/firebase.viewer",
  ])
  role = each.key
}

# The project's billing link and the budget.
resource "google_billing_account_iam_member" "terraform_plan" {
  billing_account_id = var.billing_account
  role               = "roles/billing.viewer"
  member             = google_service_account.terraform_plan.member
}

# terraform-apply applies this directory from a push to main. It manages
# IAM, its own bindings among them, so it can grant itself more: a push
# to main is trusted as much as a person running Terraform.
resource "google_service_account" "terraform_apply" {
  project      = google_project.transom.project_id
  account_id   = "terraform-apply"
  display_name = "transom Terraform apply"
  description  = "Applies gateway/ from a push to main"

  depends_on = [google_project_service.transom]
}

resource "google_service_account_iam_member" "main_acts_as_terraform_apply" {
  service_account_id = google_service_account.terraform_apply.name
  role               = "roles/iam.workloadIdentityUser"
  member             = local.pushes_to_main
}

resource "google_project_iam_member" "terraform_apply" {
  project = google_project.transom.project_id
  member  = google_service_account.terraform_apply.member

  for_each = toset([
    "roles/viewer",
    "roles/iam.securityReviewer",
    "roles/serviceusage.serviceUsageAdmin",
    "roles/firebase.admin",
    "roles/iam.serviceAccountAdmin",
    "roles/iam.serviceAccountKeyAdmin",
    "roles/resourcemanager.projectIamAdmin",
    "roles/iam.workloadIdentityPoolAdmin",
    "roles/artifactregistry.admin",
    "roles/run.admin",
    # Linking the project to the billing account, with billing.user on
    # the account below.
    "roles/billing.projectManager",
  ])
  role = each.key
}

# Cloud Run lets only someone who may act as a service's account change
# the service.
resource "google_service_account_iam_member" "terraform_apply_acts_as_gateway" {
  service_account_id = google_service_account.gateway.name
  role               = "roles/iam.serviceAccountUser"
  member             = google_service_account.terraform_apply.member
}

resource "google_billing_account_iam_member" "terraform_apply" {
  billing_account_id = var.billing_account
  member             = google_service_account.terraform_apply.member

  for_each = toset([
    # Linking a project to the account.
    "roles/billing.user",
    # The budget.
    "roles/billing.costsManager",
  ])
  role = each.key
}
