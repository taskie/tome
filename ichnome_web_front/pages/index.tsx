import Head from "next/head";
import { applicationName } from "../config";
import { NextPage } from "next";

export const HomePage: NextPage = () => {
  return (
    <div className="container">
      <Head>
        <title>{applicationName}</title>
      </Head>
      <h1>{applicationName}</h1>
    </div>
  );
};

export default HomePage;
