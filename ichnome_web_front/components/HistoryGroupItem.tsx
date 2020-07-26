import { IchHistory } from "@/api/types";
import GroupLink from "./GroupLink";
import StatLink from "./StatLink";
import FootprintLink from "./FootprintLink";

type Props = {
  workspaceName: string;
  groupName?: string;
  history: IchHistory;
};

export const HistoryGroupItem: React.FC<Props> = ({
  workspaceName,
  groupName,
  history: { group_name, path, version, mtime, digest, updated_at },
}) => {
  return (
    <li>
      {digest != null ? <FootprintLink workspaceName={workspaceName} digest={digest} length={8} /> : undefined}
      {" / "}
      {mtime != null ? mtime : "Nothing"}
      {" / "}
      {updated_at}
      {" / "}
      <GroupLink workspaceName={workspaceName} groupName={group_name ?? groupName ?? "default"} />
      {" / "}
      <StatLink workspaceName={workspaceName} groupName={group_name ?? groupName ?? "default"} path={path} />
      {" / "}
      {version}
    </li>
  );
};

export default HistoryGroupItem;
