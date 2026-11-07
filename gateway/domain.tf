# The gateway at https://wake.transom.sh/. Cloud Run maps a domain only
# for someone Google knows owns it, so Terraform proves ownership of
# transom.sh with a TXT record first. site/ owns the transom.sh zone, and
# this directory writes the records the gateway needs into it.
#
# Site Verification takes its own OAuth scope, which application-default
# credentials lack unless the login asks for it (README.md).

data "linode_domain" "transom_sh" {
  domain = "transom.sh"
}

data "google_site_verification_token" "transom_sh" {
  type                = "INET_DOMAIN"
  identifier          = "transom.sh"
  verification_method = "DNS_TXT"

  depends_on = [google_project_service.transom]
}

resource "linode_domain_record" "google_site_verification" {
  domain_id   = data.linode_domain.transom_sh.id
  name        = ""
  record_type = "TXT"
  target      = data.google_site_verification_token.transom_sh.token
  ttl_sec     = 300
}

resource "google_site_verification_web_resource" "transom_sh" {
  site {
    type       = "INET_DOMAIN"
    identifier = "transom.sh"
  }
  verification_method = "DNS_TXT"

  depends_on = [linode_domain_record.google_site_verification]
}

resource "google_cloud_run_domain_mapping" "wake" {
  project  = google_project.transom.project_id
  location = var.region
  name     = "wake.transom.sh"

  metadata {
    namespace = google_project.transom.project_id
  }

  spec {
    route_name = google_cloud_run_v2_service.gateway.name
  }

  depends_on = [google_site_verification_web_resource.transom_sh]
}

# Google serves every mapped subdomain from this name, and issues its
# certificate once the record resolves. The record waits for the mapping,
# so the name never points at Google while Google has nothing to serve.
resource "linode_domain_record" "wake" {
  domain_id   = data.linode_domain.transom_sh.id
  name        = "wake"
  record_type = "CNAME"
  target      = "ghs.googlehosted.com"
  ttl_sec     = 300

  depends_on = [google_cloud_run_domain_mapping.wake]
}
