# Signal Architecture

## Overview

Signal is a local-first human-agent handoff inbox system. It allows scripts and agents to send messages that can be viewed and replied to from a phone browser, with replies then consumed back by scripts/agents.

## Components

### signal-core

The core library containing:
- **models.rs** - Data structures (Message, Reply, Event, OutboxEntry)
- **storage.rs** - SQLite persistence layer
- **events.rs** - Event creation utilities
- **permissions.rs** - Access control logic

### signal-daemon

The HTTP server built with Axum:
- **main.rs** - CLI argument parsing and server startup
- **api.rs** - REST API handlers
- **html.rs** - Server-rendered HTML pages
- **app_state.rs** - Application state management

### signal-cli

A CLI tool for:
- Sending messages
- Listing inbox
- Fetching latest pending reply
- Creating replies

## Data Model

### Message
- `id` - UUID
- `thread_id` - UUID for grouping
- `title` - Short title
- `body` - Full message content
- `source` - Origin (codex, phone, etc.)
- `source_device` - Device identifier
- `agent_id` - Associated agent
- `project` - Project name
- `status` - new | pending | consumed | archived
- `permission_level` - private | ai_readable | actionable

### Reply
- `id` - UUID
- `message_id` - Parent message
- `body` - Reply content
- `source` - Origin
- `source_device` - Device identifier
- `status` - pending | consumed | archived

### Event
- `id` - UUID
- `event_type` - Type string
- `actor` - Actor identifier
- `created_at` - Timestamp
- `payload_json` - Event data

## API Endpoints

### Health
- `GET /health` - Returns `{ ok: true }`

### Messages
- `POST /api/messages` - Create message
- `GET /api/messages` - List messages (with filters)
- `GET /api/messages/:id` - Get message with replies

### Replies
- `POST /api/messages/:id/replies` - Create reply
- `GET /api/replies/latest` - Get latest pending reply
- `POST /api/replies/:id/consume` - Mark reply as consumed

### HTML
- `GET /` - Inbox page
- `GET /message/:id` - Message detail page

## Storage

SQLite with tables:
- `messages` - Message storage
- `replies` - Reply storage
- `events` - Event log
- `outbox` - Outgoing messages (stubbed)

## Authentication

Simple token-based auth for API:
- Pass `--token` to daemon
- Include `X-Signal-Token` header in API requests
- HTML forms remain unauthenticated for local demo

## Future Enhancements

### Phone Companion
- Native mobile app for push notifications
- Offline message sync
- Better reply UX

### Offline Cache
- Service worker for offline inbox access
- Background sync when online

### Notification Adapters
- APNs (Apple Push Notification service)
- FCM (Firebase Cloud Messaging)
- Webhooks for external triggers

### Device Pairing
- QR code pairing flow
- Per-device authentication tokens
- Encrypted message storage

### Encryption
- End-to-end encryption for messages
- Key derivation from device pairing
- Secure storage of credentials