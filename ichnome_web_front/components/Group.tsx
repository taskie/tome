import Link from "next/link";
import { uri } from "../utils/uri";
import { IchGroup } from "@/api/types";
import GroupLink from "./GroupLink";
import FootprintLink from "./FootprintLink";

type Props = {
  group: IchGroup;
};

export const Group: React.FC<Props> = ({ group: { id, type, url, digest, created_at, updated_at } }) => {
  return (
    <ul>
      <li>
        ID: <GroupLink groupId={id} /> (Stats: <GroupLink groupId={id} family={"stats"} />)
      </li>
      <li>Type: {type}</li>
      <li>URL: {url}</li>
      {digest != null ? (
        <li>
          Digest: <FootprintLink digest={digest} />
        </li>
      ) : undefined}
      <li>Group Created At: {created_at}</li>
      <li>Group Updated At: {updated_at}</li>
    </ul>
  );
};

export default Group;
