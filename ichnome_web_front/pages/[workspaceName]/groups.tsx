import { NextPage } from "next";
import Head from "next/head";
import { useRouter } from "next/router";
import { uria } from "@/utils/uri";
import { defaultInstance } from "@/api/apiClient";
import { applicationName } from "@/config";
import { GetGroupsResponse } from "@/api/types";
import Group from "@/components/Group";
import GlobalNav from "@/components/GlobalNav";

type Query = { workspaceName: string };

type Response = GetGroupsResponse;

type Props = { response?: Response; err?: string };

const ResponseView: React.FC<{ response: Response; workspaceName: string }> = ({
  workspaceName,
  response: { groups },
}) => {
  return (
    <>
      {groups.map((n) => (
        <Group key={n.id} workspaceName={workspaceName} group={n} />
      ))}
    </>
  );
};

export const GroupPage: NextPage<Props> = (props) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const { workspaceName } = (rawQuery as unknown) as Query;
  const pageTitle = `Group Definitions: ${workspaceName}`;
  return (
    <div className="container">
      <Head>
        <title>
          {pageTitle} - {applicationName}
        </title>
      </Head>
      <GlobalNav workspaceName={workspaceName} />
      <h1>{pageTitle}</h1>
      {props.response != null ? (
        <ResponseView response={props.response} workspaceName={workspaceName} />
      ) : (
        <p>Some error occured: {props.err}</p>
      )}
    </div>
  );
};

GroupPage.getInitialProps = async ({ query: rawQuery }) => {
  try {
    const { workspaceName } = (rawQuery as unknown) as Query;
    const path = uria`${workspaceName}/groups`;
    const { data } = await defaultInstance.get(path);
    return { response: data };
  } catch (err) {
    // console.error(err);
    return { err: err.message };
  }
};

export default GroupPage;
