import logging
from typing import AsyncGenerator

from aiogram import Bot
from aiogram.client.default import DefaultBotProperties
from aiogram.types import BotCommand
from dishka import Provider, Scope
from opentelemetry import trace

from telegram_client.config import TelegramConfig

logger = logging.getLogger(__name__)
tracer = trace.get_tracer(__name__)

bot_provider = Provider(scope=Scope.APP)


@bot_provider.provide
async def get_bot(config: TelegramConfig) -> AsyncGenerator[Bot, BaseException]:
    async with Bot(token=config.bot_token, default=DefaultBotProperties()) as bot:
        await bot.set_my_commands(
            [
                BotCommand(command="start", description="Start the bot"),
            ]
        )
        yield bot
