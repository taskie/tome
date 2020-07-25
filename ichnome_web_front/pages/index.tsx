import Head from "next/head";
import Link from "next/link";
import { applicationName } from "../config";
import { defaultInstance } from "../api/apiClient";
import { NextPage } from "next";
import { GetGroupsResponse } from "@/api/types";
import GroupLink from "@/components/GroupLink";

type Response = GetGroupsResponse;

type Props = { response?: Response; err?: string };

export const HomePage: NextPage<Props> = (props) => {
  return (
    <div className="container">
      <Head>
        <title>{applicationName}</title>
      </Head>
      <main>
        <h1 className="title">{applicationName}</h1>
        <h2>Groups</h2>
        <ul>
          {props.response?.groups.map((n) => (
            <li>
              <GroupLink key={n.id} groupId={n.id} family="stats" />
            </li>
          ))}
        </ul>
        <p>
          <Link href="/groups">
            <a>List</a>
          </Link>
        </p>
      </main>
    </div>
  );
};

HomePage.getInitialProps = async () => {
  try {
    const path = "groups";
    const { data } = await defaultInstance.get(path);
    return { response: data };
  } catch (err) {
    console.error(err);
    return { err: err.message };
  }
};

export default HomePage;
