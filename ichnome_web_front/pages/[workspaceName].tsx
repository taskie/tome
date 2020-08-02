import Head from "next/head";
import Link from "next/link";
import { applicationName } from "../config";
import { defaultInstance } from "../api/apiClient";
import { NextPage } from "next";
import { GetGroupsResponse } from "@/api/types";
import GroupLink from "@/components/GroupLink";
import { useRouter } from "next/router";
import { uria } from "@/utils/uri";

type Query = {
  workspaceName: string;
};

type Response = GetGroupsResponse;

type Props = { response?: Response; err?: string };

export const HomePage: NextPage<Props> = (props) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const { workspaceName } = (rawQuery as unknown) as Query;
  return (
    <div className="container">
      <Head>
        <title>
          {workspaceName} - {applicationName}
        </title>
      </Head>
      <main>
        <h1 className="title">
          {workspaceName} - {applicationName}
        </h1>
        <h2>Groups</h2>
        <ul>
          {props.response?.groups.map((g) => (
            <li key={g.id}>
              <GroupLink workspaceName={workspaceName} groupName={g.name} family="stats" />
            </li>
          ))}
        </ul>
        <p>
          <Link href={uria`/${workspaceName}/groups`}>
            <a>List</a>
          </Link>
        </p>
      </main>
    </div>
  );
};

HomePage.getInitialProps = async ({ query: rawQuery }) => {
  try {
    const { workspaceName } = (rawQuery as unknown) as Query;
    const path = `${workspaceName}/groups`;
    const { data } = await defaultInstance.get(path);
    return { response: data };
  } catch (err) {
    // console.error(err);
    return { err: err.message };
  }
};

export default HomePage;
