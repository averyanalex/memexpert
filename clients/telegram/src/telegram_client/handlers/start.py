from aiogram import Bot, Router
from aiogram.filters import CommandStart
from aiogram.types import Message
from aiogram.utils.formatting import Code, Text

start_router = Router()


@start_router.message(CommandStart())
async def start_handler(message: Message, bot: Bot) -> None:
    me = await bot.get_me()
    await message.answer(
        **Text(
            "Добро пожаловать в Telegram-клиент MemeXpert! Чтобы найти мемы, просто напишите ",
            Code(f"@{me.username} запрос"),
            " в любом чате.",
        ).as_kwargs(),
    )
