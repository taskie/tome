import Head from "next/head";
import Link from "next/link";
import { applicationName } from "../config";
import { defaultInstance } from "../api/apiClient";
import { NextPage } from "next";
import { GetGroupsResponse } from "@/api/types";
import GroupLink from "@/components/GroupLink";

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
