variable "foo" {
  type        = list(string)
  description = "you know the drill"
  default     = ["bar", "baz"]
}

configuration {
  for_each = toset(var.foo)

  service_name = "${each.key}-service"
}
