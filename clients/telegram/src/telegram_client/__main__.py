import asyncio

from aiogram import Bot, Dispatcher
from dishka.integrations.aiogram import setup_dishka

from telegram_client import dishka_container
from telegram_client.handlers import main_router


async def run_app() -> None:
    bot = await dishka_container.get(Bot)

    dispatcher = Dispatcher()

    setup_dishka(container=dishka_container, router=dispatcher, auto_inject=True)
    dispatcher.shutdown.register(dishka_container.close)

    dispatcher.include_router(main_router)

    await dispatcher.start_polling(bot)


def main() -> None:
    asyncio.run(run_app())


if __name__ == "__main__":
    main()
