const path = require("path");
const HtmlWebpackPlugin = require("html-webpack-plugin");
const MiniCssExtractPlugin = require("mini-css-extract-plugin");
const { CleanWebpackPlugin } = require("clean-webpack-plugin");
const { DefinePlugin } = require("webpack");

const PUBLIC = "webrtc";

module.exports = ({ slow, standalone }, { mode }) => ({
  entry: {
    main: ["./src/index.tsx", "./src/style.css"],
  },
  devtool: "source-map",
  devServer: {
    proxy: {
      [`/${PUBLIC}/signalling`]: {
        target: "ws://localhost:4000",
        pathRewrite: { [`^/${PUBLIC}/signalling`]: "" },
        ws: true,
      },
    },
    publicPath: "/" + PUBLIC
  },
  module: {
    rules: [
      {
        test: /\.tsx?$/,
        use: "ts-loader",
        exclude: /node_modules/,
      },
      {
        test: /\.css$/,
        use: [
          mode === "production" ? MiniCssExtractPlugin.loader : "style-loader",
          "css-loader",
        ],
      },
    ],
  },
  resolve: {
    extensions: [".tsx", ".ts", ".js"],
  },
  plugins: [
    new MiniCssExtractPlugin(),
    new CleanWebpackPlugin(),
    new HtmlWebpackPlugin({
      template: "src/index.html",
    }),
    new DefinePlugin({
      PUBLIC: JSON.stringify(PUBLIC),
    }),
  ],
  output: {
    path: path.resolve(__dirname, "dist"),
    publicPath: "/" + PUBLIC + "/",
  },
});
