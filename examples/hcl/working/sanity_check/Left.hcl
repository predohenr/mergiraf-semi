variable "foo" {
  type        = string
  description = "you know the drill"
  default     = "bar"
}

configuration {
  service_name = "${var.foo}-service"
}
