variable "foo" {
  type        = string
  description = "you know the drill"
  default     = "baz"
}

configuration {
  service_name = "${var.foo}-service"
}
