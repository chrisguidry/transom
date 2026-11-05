# The Firebase project that the official app receives wake-ups through,
# and the gateway's credentials to send them.
#
# Run Terraform from this directory, signed in to Google as a person
# (README.md). The state lives in the Linode bucket
# transom-terraform-state, beside site/'s, and it holds the gateway's
# service account key. .envrc.private, which is gitignored, exports
# AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY from the 1Password item
# "transom Terraform state (Linode Object Storage)".
terraform {
  required_version = ">= 1.10"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 8.10"
    }
    google-beta = {
      source  = "hashicorp/google-beta"
      version = "~> 8.10"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 3.4"
    }
  }

  backend "s3" {
    bucket       = "transom-terraform-state"
    key          = "gateway.tfstate"
    region       = "us-east-1"
    use_lockfile = true

    endpoints = {
      s3 = "https://us-east-1.linodeobjects.com"
    }

    skip_credentials_validation = true
    skip_region_validation      = true
    skip_requesting_account_id  = true
    skip_metadata_api_check     = true
    skip_s3_checksum            = true
  }
}

# A person's credentials have no project of their own to bill API quota
# to, so these providers bill each call to the project it acts on.
provider "google" {
  user_project_override = true
}

provider "google-beta" {
  user_project_override = true
}

# The project can't take quota checks until it exists and has Service
# Usage turned on, so it and its APIs are made without the override.
provider "google-beta" {
  alias                 = "no_user_project_override"
  user_project_override = false
}

provider "kubernetes" {
  config_path    = var.kubeconfig_path
  config_context = var.kube_context
}

# FCM is free on Firebase's no-cost plan, so the project has no billing
# account. It has no organization either, since it belongs to a person.
resource "google_project" "transom" {
  provider = google-beta.no_user_project_override

  project_id = var.project_id
  name       = "transom"

  # The Firebase console lists only the projects with this label.
  labels = {
    "firebase" = "enabled"
  }
}

resource "google_project_service" "transom" {
  provider = google-beta.no_user_project_override
  project  = google_project.transom.project_id

  for_each = toset([
    # The project bills its own quota from here on.
    "serviceusage.googleapis.com",
    "cloudresourcemanager.googleapis.com",
    "firebase.googleapis.com",
    # The gateway sends through FCM's HTTP v1 API.
    "fcm.googleapis.com",
    # The app's FCM SDK registers through these to get its token.
    "fcmregistrations.googleapis.com",
    "firebaseinstallations.googleapis.com",
    # The gateway's service account and its key.
    "iam.googleapis.com",
  ])
  service = each.key

  # Turning an API off breaks whatever still uses it, and leaving one on
  # costs nothing.
  disable_on_destroy = false
}

resource "google_firebase_project" "transom" {
  provider = google-beta
  project  = google_project.transom.project_id

  depends_on = [google_project_service.transom]
}

resource "google_firebase_android_app" "transom" {
  provider = google-beta
  project  = google_firebase_project.transom.project

  display_name = "transom"
  package_name = "sh.transom"
  # Firebase takes lowercase hex without separators, and keytool and
  # assetlinks.json print uppercase pairs separated by colons.
  sha256_hashes = [
    for fingerprint in var.android_signing_sha256 :
    lower(replace(fingerprint, ":", ""))
  ]
}

# The google-services.json that Firebase writes for the app, read here
# for the API key, which the app resource names only by its id.
data "google_firebase_android_app_config" "transom" {
  provider = google-beta
  project  = google_firebase_android_app.transom.project
  app_id   = google_firebase_android_app.transom.app_id
}

resource "google_service_account" "gateway" {
  project      = google_project.transom.project_id
  account_id   = "gateway"
  display_name = "transom gateway"
  description  = "Sends wake-ups to the official app through FCM"

  depends_on = [google_project_service.transom]
}

# This role grants cloudmessaging.messages.create, which sending a
# message needs, and topic subscriptions besides. No predefined role
# grants less.
resource "google_project_iam_member" "gateway_sends_messages" {
  project = google_project.transom.project_id
  role    = "roles/firebasecloudmessaging.admin"
  member  = google_service_account.gateway.member
}

resource "google_service_account_key" "gateway" {
  service_account_id = google_service_account.gateway.name
}

# The gateway reads this key at startup. Until the Secret exists, it
# answers 503 and phones keep their connection to their server.
resource "kubernetes_secret_v1" "gateway_fcm" {
  metadata {
    name      = "gateway-fcm"
    namespace = "transom"
  }

  data = {
    "service-account.json" = base64decode(google_service_account_key.gateway.private_key)
  }
}
