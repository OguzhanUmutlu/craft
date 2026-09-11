FROM golang:1.22-alpine AS builder
WORKDIR /app
COPY go.mod ./
COPY main.go install.sh install.ps1 ./
RUN CGO_ENABLED=0 go build -ldflags="-s -w" -o craft-server main.go

FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata
WORKDIR /app
COPY --from=builder /app/craft-server /usr/local/bin/craft-server
VOLUME ["/releases"]
EXPOSE 8080
ENTRYPOINT ["craft-server"]
CMD ["--port", "8080", "--dir", "/releases"]
