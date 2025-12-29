.PHONY: build push

build:
	docker buildx build --platform linux/amd64,linux/arm64 -t zitronenjoghurt/mcdc-bot:latest -f docker/Dockerfile .

push:
	docker buildx build --platform linux/amd64,linux/arm64 -t zitronenjoghurt/mcdc-bot:latest -f docker/Dockerfile --push .