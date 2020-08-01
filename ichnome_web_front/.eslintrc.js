module.exports = {
  root: true,
  env: {
    node: true,
    es6: true,
  },
  extends: [
    "eslint:recommended",
    "plugin:@typescript-eslint/recommended",
    "plugin:prettier/recommended",
    "prettier/@typescript-eslint",
  ],
  plugins: ["@typescript-eslint", "eslint-plugin-react"],
  parser: "@typescript-eslint/parser",
  parserOptions: {
    sourceType: "module",
  },
  rules: {
    "@typescript-eslint/explicit-function-return-type": "off",
    "@typescript-eslint/no-explicit-any": "off",
    "@typescript-eslint/no-unused-vars": ["warn", { args: "none" }],
    "@typescript-eslint/no-use-before-define": "off",
    "@typescript-eslint/prefer-interface": "off",
    eqeqeq: ["warn", "smart"],
    "linebreak-style": ["warn", "unix"],
    "no-console": process.env.NODE_ENV === "production" ? "error" : "warn",
    "no-debugger": process.env.NODE_ENV === "production" ? "error" : "warn",
    "no-use-before-define": "off",
    "prefer-const": ["warn"],
    "prettier/prettier": [
      "warn",
      {
        printWidth: 120,
        trailingComma: "all",
        semi: true,
      },
    ],
    quotes: ["warn", "double"],
    "react/jsx-uses-react": 1,
    "react/jsx-uses-vars": 1,
  },
};
