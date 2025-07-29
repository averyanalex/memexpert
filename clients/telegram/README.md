# MemeXpert Telegram Bot

A Telegram bot client for MemeXpert - an intelligent meme management system that allows users to create memes with AI-generated Russian tags and search through them seamlessly.

## 🚀 Features

- **AI-Powered Meme Creation**: Send photos to generate Russian-language tags using advanced AI
- **Inline Meme Search**: Search for memes directly in any chat using inline queries
- **Smart Tag Generation**: Leverages MemeXpert backend for intelligent tag creation
- **Russian Language Interface**: Fully localized for Russian-speaking users
- **Real-time Processing**: Instant photo processing with typing indicators
- **Clean Architecture**: Built with modern async patterns and dependency injection
- **Observability**: Integrated monitoring and tracing capabilities

## 🏗️ Architecture

The project follows a clean architecture pattern:

```text
src/telegram_client/
├── __init__.py        # DI container setup and observability
├── __main__.py        # Bot startup and polling
├── bot.py             # Bot configuration and commands
├── config.py          # Configuration models
├── handlers/          # Message and query handlers
│   ├── __init__.py    # Router registration
│   ├── start.py       # Welcome message handler
│   ├── create.py      # Photo processing handler
│   └── search.py      # Inline search handler
└── clients/           # External service clients
    ├── __init__.py    # Client exports
    └── backend.py     # MemeXpert backend client
```

### Key Components

- **Handlers**: Specialized handlers for different user interactions
- **Backend Client**: HTTP client for MemeXpert backend API integration
- **Bot Configuration**: Aiogram-based bot setup with command registration
- **Dependency Injection**: Dishka container for clean separation of concerns
- **Configuration**: Pydantic Settings for type-safe configuration management
- **Observability**: OpenTelemetry integration for monitoring and tracing

## 🛠️ Tech Stack

- **Bot Framework**: Aiogram 3.21+
- **Dependency Injection**: Dishka 1.6+
- **Package Management**: uv
- **Python Version**: 3.13+

## 📋 Prerequisites

- Python 3.13+
- MemeXpert Backend API running
- Telegram Bot Token from @BotFather
- uv package manager

## 🚀 Quick Start

### 1. Create bot

Create a new bot in [@BotFather](https://t.me/BotFather) and enable inline mode.

### 2. Clone and Setup

```bash
# Install dependencies
uv sync

# Copy environment configuration
cp .env.example .env
```

### 3. Configure Environment

Set bot token obtained from @BotFather in `.env`.

### 4. Start the Bot

```bash
uv run python -m telegram_client
```

The bot will start polling for messages and be ready to use!

## 🤖 Bot Usage

### Creating Memes

1. Start a chat with your bot
2. Send any photo
3. The bot will process it and return AI-generated Russian tags
4. Your meme is now stored and searchable

### Searching Memes

1. In any chat (including groups), type `@yourbotname query`
2. The bot will show inline results with matching memes
3. Select a meme to share it instantly

### Commands

- `/start` - Welcome message and usage instructions

## 🔧 Development

### Code Quality

```bash
# Type checking
uv run mypy src/

# Linting and formatting
uv run ruff check src/
uv run ruff format src/
```

## 🔍 Key Features Deep Dive

### Meme Creation Flow

When a user sends a photo:

1. **Photo Processing**: Bot receives photo message and extracts the highest quality version
2. **Download**: Downloads photo data from Telegram servers
3. **Backend Integration**: Sends photo to MemeXpert backend for AI tag generation
4. **Response**: Returns generated Russian tags to the user
5. **Storage**: Meme is stored in backend with unique identifiers for future search

### Inline Search

The bot supports Telegram's inline query feature:

1. **Query Processing**: Receives inline queries in real-time
2. **Backend Search**: Searches stored memes using text matching
3. **Result Display**: Returns up to 50 matching memes as cached photos
4. **Instant Sharing**: Users can select and share memes directly

### Backend Integration

The bot integrates with MemeXpert backend through:

- **HTTP Client**: Async HTTPX client with timeout and tracing
- **API Methods**:
  - `create_meme()` - Process and store new memes
  - `search_memes()` - Search existing memes by text
- **Error Handling**: Robust error handling for network issues
- **Observability**: Request tracing and monitoring

## 🌟 Example Interaction

```text
User: [sends photo of a cat]
Bot: Мем создан!

Сгенерированные теги:
• кот
• милый
• животное
• домашний
• пушистый
```

```text
User: @yourbot кот
Bot: [shows inline results with cat memes]
```
