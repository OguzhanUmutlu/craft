.PHONY: all build run clean install

all: build

build:
	go build -ldflags="-s -w" -o craft-server main.go

run: build
	./craft-server --port 8080

install: build
	install -d /opt/craft-distribution/bin /opt/craft-distribution/releases
	install -m 755 craft-server /opt/craft-distribution/bin/craft-server
	install -m 644 craft-server.service /etc/systemd/system/craft-server.service
	systemctl daemon-reload
	systemctl enable --now craft-server
	@echo "✓ Craft Distribution Server installed and running via systemd!"

clean:
	rm -f craft-server
