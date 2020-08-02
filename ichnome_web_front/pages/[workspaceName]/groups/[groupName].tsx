import { NextPage } from "next";
import Head from "next/head";
import { useRouter } from "next/router";
import { uria } from "@/utils/uri";
import { defaultInstance } from "@/api/apiClient";
import { applicationName } from "@/config";
import { GetGroupResponse } from "@/api/types";
import Group from "@/components/Group";
import Stat from "@/components/Stat";
import FootprintView from "@/components/Footprint";
import HistoryGroup from "@/components/HistoryGroup";
import GlobalNav from "@/components/GlobalNav";

type Query = {
  workspaceName: string;
  groupName: string;
};

type Response = GetGroupResponse;

type Props = { response?: Response; err?: string };

const ResponseView: React.FC<{ response: Response; workspaceName: string; groupName: string }> = ({
  response: { group, stat, histories, footprints },
  workspaceName,
  groupName,
}) => {
  const footprint = stat != null && footprints != null ? footprints["" + stat.footprint_id] : undefined;
  return (
    <>
      <h2>Group</h2>
      <Group workspaceName={workspaceName} group={group} />
      <h2>Stat</h2>
      {stat != null ? <Stat workspaceName={workspaceName} groupName={groupName} stat={stat} /> : "Nothing"}
      {histories != null ? (
        <>
          <h2>Histories</h2>
          <HistoryGroup workspaceName={workspaceName} groupName={groupName} histories={histories} />
        </>
      ) : undefined}
      {footprint != null ? (
        <>
          <h2>Footprint</h2>
          <FootprintView workspaceName={workspaceName} footprint={footprint} />
        </>
      ) : undefined}
    </>
  );
};

export const GroupPage: NextPage<Props> = (props) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const { workspaceName, groupName } = (rawQuery as unknown) as Query;
  const pageTitle = `Group Definition: ${groupName}`;
  return (
    <div className="container">
      <Head>
        <title>
          {pageTitle} - {applicationName}
        </title>
      </Head>
      <GlobalNav workspaceName={workspaceName} groupName={groupName} />
      <h1>{pageTitle}</h1>
      {props.response != null ? (
        <ResponseView response={props.response} workspaceName={workspaceName} groupName="__meta" />
      ) : (
        <p>Some error occured: {props.err}</p>
      )}
    </div>
  );
};

GroupPage.getInitialProps = async ({ query: rawQuery }) => {
  try {
    const { workspaceName, groupName } = (rawQuery as unknown) as Query;
    const path = uria`${workspaceName}/groups/${groupName}`;
    const { data } = await defaultInstance.get(path);
    return { response: data };
  } catch (err) {
    // console.error(err);
    return { err: err.message };
  }
};

export default GroupPage;
