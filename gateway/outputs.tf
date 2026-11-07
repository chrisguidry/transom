# The values the app is built with. None of them is secret: Firebase
# puts all of them in every app it configures.

locals {
  google_services = jsondecode(base64decode(
    data.google_firebase_android_app_config.transom.config_file_contents
  ))
}

output "project_id" {
  description = "transom.firebase.projectId"
  value       = google_project.transom.project_id
}

output "application_id" {
  description = "transom.firebase.applicationId"
  value       = google_firebase_android_app.transom.app_id
}

output "api_key" {
  description = "transom.firebase.apiKey"
  value       = local.google_services.client[0].api_key[0].current_key
}

output "sender_id" {
  description = "transom.firebase.senderId"
  value       = google_project.transom.number
}

# The four values as lines of a gradle.properties file.
output "gradle_properties" {
  description = "The transom.firebase.* Gradle properties"
  value       = <<-EOT
    transom.firebase.projectId=${google_project.transom.project_id}
    transom.firebase.applicationId=${google_firebase_android_app.transom.app_id}
    transom.firebase.apiKey=${local.google_services.client[0].api_key[0].current_key}
    transom.firebase.senderId=${google_project.transom.number}
  EOT
}

# The values the gateway's workflow deploys with. None of them is secret.

output "gateway_url" {
  description = "The gateway's run.app URL"
  value       = google_cloud_run_v2_service.gateway.uri
}

output "gateway_image" {
  description = "The gateway's image, as Cloud Run pulls it, without a tag"
  value       = local.gateway_image
}

output "workload_identity_provider" {
  description = "The provider GitHub Actions authenticates through"
  value       = google_iam_workload_identity_pool_provider.github.name
}

output "deployer" {
  description = "The service account GitHub Actions deploys as"
  value       = google_service_account.deployer.email
}
