# A budget on the gateway's project. The gateway's traffic fits in the
# free tiers of Cloud Run and Artifact Registry, so any spending at all is
# a surprise worth an email. Google sends the alerts to the billing
# account's administrators.
resource "google_billing_budget" "transom" {
  billing_account = var.billing_account
  display_name    = "transom-wake"

  budget_filter {
    projects = ["projects/${google_project.transom.number}"]
  }

  amount {
    specified_amount {
      currency_code = "USD"
      units         = "1"
    }
  }

  threshold_rules {
    threshold_percent = 0.5
  }

  threshold_rules {
    threshold_percent = 1.0
  }

  depends_on = [google_project_service.transom]
}
