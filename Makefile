.PHONY: build push

IMAGE := zitronenjoghurt/mcdc-bot
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
BUILD := docker buildx build --platform linux/amd64,linux/arm64 -t $(IMAGE):$(VERSION) -t $(IMAGE):latest -f docker/Dockerfile

build:
	$(BUILD) .

push:
	$(BUILD) --push .
