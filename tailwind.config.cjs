/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./templates/*.html"],
  theme: {
    extend: {
      fontFamily: {
        sans: ["Inter", "Roboto", "Helvetica Neue", "Arial Nova", "Nimbus Sans", "Arial", "sans-serif"],
      },
      minWidth: {
        '1/2': '50%',
      },
    },
  },
  plugins: [],
};
