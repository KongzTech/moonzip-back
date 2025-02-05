group "default" {
  targets = ["backend"]
}

target "backend" {
  context = "."
  dockerfile = "docker/Dockerfile.ci"
  args = {
    MOONZIP_AUTHORITY = "mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN"
  }
  cache-from = [
    "type=local,src=/builds/mzip/backend"
  ]
  cache-to = [
    "type=local,dest=/builds/mzip/backend"
  ]
  output = ["type=docker"]
  tags = ["moonzip/dev:latest"]
  target = "dev"
}