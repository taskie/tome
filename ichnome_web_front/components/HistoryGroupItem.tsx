import { IchHistory } from "@/api/types";
import GroupLink from "./GroupLink";
import StatLink from "./StatLink";
import FootprintLink from "./FootprintLink";

type Props = {
  history: IchHistory;
};

export const HistoryGroupItem: React.FC<Props> = ({
  history: { group_id, path, version, mtime, digest, updated_at },
}) => {
  return (
    <li>
      {digest != null ? <FootprintLink digest={digest} length={8} /> : undefined}
      {" / "}
      {mtime != null ? mtime : "Nothing"}
      {" / "}
      {updated_at}
      {" / "}
      <GroupLink groupId={group_id} />
      {" / "}
      <StatLink groupId={group_id} path={path} />
      {" / "}
      {version}
    </li>
  );
};

export default HistoryGroupItem;
