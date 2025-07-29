# MemeXpert Agent

A FastAPI-based AI service for MemeXpert - an intelligent tag generation microservice that automatically creates Russian-language tags for images using advanced computer vision and natural language processing.

## 🚀 Features

- **AI-Powered Tag Generation**: Uses Google Gemini Flash 2.5 or other VLM for intelligent image analysis
- **Russian Language Tags**: Specialized in generating Russian, lowercase, concise tags
- **Context-Aware Processing**: Considers existing tags to maintain consistency and avoid duplicates
- **Image Processing**: Automatically resizes and optimizes images (max 512px, JPEG compression)
- **RESTful API**: Clean FastAPI-based REST API with automatic documentation
- **Async Architecture**: Full async/await support for high performance
- **Dependency Injection**: Uses Dishka for clean architecture and testability
- **Observability**: Built-in OpenTelemetry instrumentation for monitoring
- **Type Safety**: Full type annotations with Pydantic models

## 🏗️ Architecture

The project follows a clean architecture pattern:

```text
src/agent/
├── __init__.py  # FastAPI app setup and DI container
├── agent.py     # AI agent definition and logic
├── config.py    # Configuration models
├── images.py    # Image processing utilities
└── router.py    # API route handlers
```

### Key Components

- **Agent**: Pydantic-AI powered agent for tag generation with context awareness
- **Image Processing**: PIL-based image optimization and format conversion
- **Dependency Injection**: Dishka container for clean separation of concerns
- **Configuration**: Pydantic Settings for type-safe configuration management
- **Observability**: OpenTelemetry integration for monitoring and tracing

## 🛠️ Tech Stack

- **Framework**: FastAPI
- **AI Framework**: Pydantic-AI
- **Image Processing**: Pillow (PIL)
- **Dependency Injection**: Dishka
- **Package Management**: uv
- **Python Version**: 3.13+

## 📋 Prerequisites

- Python 3.13+
- OpenRouter API key (or other LLM provider)
- uv package manager

## 🚀 Quick Start

### 1. Clone and Setup

```bash
# Install dependencies
uv sync

# Copy environment configuration
cp .env.example .env
```

### 2. Configure Environment

Edit `.env` with your settings:

```env
APP__LLM__MODEL=openrouter:google/gemini-2.5-flash
OPENROUTER_API_KEY=your-openrouter-api-key-here
```

### 3. Start the Server

```bash
# Development server
uv run uvicorn --port 8050 agent:app
```

The API will be available at `http://localhost:8050`

## 📚 API Documentation

### Endpoints

#### Generate Tags

```http
POST /generate/tags
Content-Type: application/json

{
  "existing_tags": ["мем", "кот"],
  "image": "base64_encoded_image_data"
}
```

**Response:**

```json
{
  "tags": [
    "мем",
    "кот", 
    "смешно",
    "интернет",
    "юмор"
  ]
}
```

**Constraints:**

- Image size: 1 byte to 5MB
- Supported formats: Any format supported by PIL (automatically converted to JPEG)
- Output: 10-20 Russian tags, lowercase, short and concise

### Interactive Documentation

- **Swagger UI**: `http://localhost:8050/docs`
- **ReDoc**: `http://localhost:8050/redoc`
- **OpenAPI Spec**: `http://localhost:8050/openapi.json`

## 🧪 Development

### Code Quality

```bash
# Type checking
uv run mypy src/

# Linting and formatting
uv run ruff check src/
uv run ruff format src/
```

## 🔍 Key Features Deep Dive

### AI-Powered Tag Generation

When an image is processed, the system:

1. Receives image data and optional existing tags as context
2. Processes and optimizes the image (resize to max 512px, convert to JPEG)
3. Sends the processed image to the configured AI model
4. Applies existing tags context to maintain consistency
5. Returns 10-20 relevant Russian tags, lowercase and concise

### Image Processing Pipeline

The image processing includes:

- **Format Normalization**: Converts any supported format to JPEG
- **Size Optimization**: Resizes images to maximum 512px while maintaining aspect ratio
- **Quality Control**: Applies 90% JPEG quality for optimal size/quality balance
- **Memory Efficiency**: Processes images in-memory without temporary files

### Context-Aware Processing

The agent considers existing tags to:

- Maintain consistency across similar images
- Avoid generating duplicate or conflicting tags
- Improve relevance by understanding the current tag ecosystem
- Enhance semantic coherence in tag generation

## ⚙️ Configuration Options

### Environment Variables

- `APP__LLM__MODEL`: The LLM model to use (example: `openrouter:google/gemini-2.5-flash`)
- `OPENROUTER_API_KEY`: Your OpenRouter API key for accessing LLM services

### Supported Models

The agent supports various LLM providers through Pydantic-AI:

- OpenRouter models (Google Gemini, Claude, etc.)
- OpenAI models
- Anthropic models
- Local models via Ollama
