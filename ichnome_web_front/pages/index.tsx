import Head from "next/head";
import { applicationName } from "../config";
import { NextPage } from "next";
import { GetGroupsResponse } from "@/api/types";

type Response = GetGroupsResponse;

export const HomePage: NextPage = () => {
  return (
    <div className="container">
      <Head>
        <title>{applicationName}</title>
      </Head>
    </div>
  );
};

export default HomePage;
