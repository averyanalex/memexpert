from aiogram import Router

from .create import create_router
from .search import search_router
from .start import start_router

main_router = Router()
main_router.include_router(start_router)
main_router.include_router(create_router)
main_router.include_router(search_router)
