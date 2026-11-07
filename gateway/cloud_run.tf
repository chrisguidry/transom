# The gateway, on Cloud Run, where it runs as the gateway's service
# account and signs in to FCM through the metadata server, with no key.

# Cloud Run pulls images from Artifact Registry and Docker Hub, not from
# ghcr.io, so this repository proxies ghcr.io, where CI publishes the gateway's image, and caches each
# image it pulls.
resource "google_artifact_registry_repository" "ghcr" {
  project       = google_project.transom.project_id
  location      = var.region
  repository_id = "ghcr"
  description   = "ghcr.io, for the gateway's image"
  format        = "DOCKER"
  mode          = "REMOTE_REPOSITORY"

  remote_repository_config {
    description = "ghcr.io"

    docker_repository {
      custom_repository {
        uri = "https://ghcr.io"
      }
    }
  }

  depends_on = [google_project_service.transom]
}

locals {
  # Where Cloud Run pulls the gateway's image from: ghcr.io's
  # ghcr.io/chrisguidry/transom-gateway, through the repository above.
  gateway_image = join("/", [
    "${var.region}-docker.pkg.dev",
    google_project.transom.project_id,
    google_artifact_registry_repository.ghcr.repository_id,
    "chrisguidry/transom-gateway",
  ])
}

resource "google_cloud_run_v2_service" "gateway" {
  project  = google_project.transom.project_id
  location = var.region
  name     = "gateway"
  ingress  = "INGRESS_TRAFFIC_ALL"

  template {
    service_account = google_service_account.gateway.email

    # The gateway limits each phone's wake-ups in memory, so a second
    # instance would give every phone a second allowance.
    scaling {
      min_instance_count = 0
      max_instance_count = 1
    }

    containers {
      image = "${local.gateway_image}:${var.first_image_tag}"

      ports {
        container_port = 8080
      }

      resources {
        limits = {
          cpu    = "1"
          memory = "128Mi"
        }
        # CPU only while a request is in flight, so an idle gateway costs
        # nothing.
        cpu_idle = true
      }
    }
  }

  lifecycle {
    # CI deploys each build's image, and gcloud records itself as the
    # client that did.
    ignore_changes = [
      template[0].containers[0].image,
      client,
      client_version,
    ]
  }

  depends_on = [google_project_service.transom]
}

# Any transom server may ask the gateway to wake a phone.
resource "google_cloud_run_v2_service_iam_member" "anyone_invokes_gateway" {
  project  = google_cloud_run_v2_service.gateway.project
  location = google_cloud_run_v2_service.gateway.location
  name     = google_cloud_run_v2_service.gateway.name
  role     = "roles/run.invoker"
  member   = "allUsers"
}
